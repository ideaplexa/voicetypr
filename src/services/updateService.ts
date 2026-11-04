import { relaunch } from "@tauri-apps/plugin-process";
import { message } from "@tauri-apps/plugin-dialog";
import {
  sendNotification,
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import type { AppSettings, UpdateChannel } from "@/types";
import { createLogger } from "@/lib/logger";
import { openReleaseLink, releasePageUrl } from "@/lib/releaseNotes";
import { isStoreDistribution, type DistributionInfo } from "@/types/distribution";

const log = createLogger("update");

const UPDATE_CHECK_INTERVAL = 24 * 60 * 60 * 1000; // 24 hours in milliseconds
const LAST_UPDATE_CHECK_KEY = "last_update_check";
const JUST_UPDATED_KEY = "just_updated_version";

export interface AppUpdateInfo {
  version: string;
  body: string;
  channel: UpdateChannel;
}

/** What the UI shows about updates: sidebar footer, Home and Settings → General. */
export type InstallState = "idle" | "prompting" | "installing" | "restart_pending";

export interface UpdateStatus {
  checking: boolean;
  available: AppUpdateInfo | null;
  lastChecked: number | null;
  result: "idle" | "current" | "error" | "store";
  /** One shared install state: every Install control reads it, so installs never overlap. */
  install: InstallState;
  /** The version that is installed and waiting for a restart (`install === "restart_pending"`). */
  restartVersion: string | null;
}

type CheckOutcome =
  | { kind: "update"; update: AppUpdateInfo }
  | { kind: "current" }
  | { kind: "error" }
  | { kind: "store" }
  | { kind: "skipped" }
  | { kind: "stale" };

export class UpdateService {
  private static instance: UpdateService;
  private status: UpdateStatus = {
    checking: false,
    available: null,
    lastChecked: null,
    result: "idle",
    install: "idle",
    restartVersion: null,
  };
  private listeners = new Set<() => void>();
  /** The one in-flight check. Startup, tray, manual, footer and channel-change checks all join it. */
  private pendingCheck: Promise<CheckOutcome> | null = null;
  /** Bumped when the update channel changes; a check started under an older epoch is dropped. */
  private channelEpoch = 0;
  private channel: UpdateChannel | null = null;
  private updateCheckTimer: number | null = null;
  private isSessionActive = false;
  private pendingRelaunch = false;
  private pendingUpdateVersion: string | null = null;
  private distributionInfo: DistributionInfo | null = null;
  private distributionInfoPromise: Promise<DistributionInfo> | null = null;

  private constructor() {}

  getStatus = (): UpdateStatus => this.status;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  private setStatus(patch: Partial<UpdateStatus>): void {
    this.status = { ...this.status, ...patch };
    this.listeners.forEach((listener) => listener());
  }

  /**
   * One check at a time. A caller that arrives while a check is running joins it, so nobody sees a
   * half-finished snapshot. `background` honours the 24 h interval and resolves `skipped` without
   * touching the status.
   */
  private runCheck(background: boolean): Promise<CheckOutcome> {
    if (this.pendingCheck) return this.pendingCheck;
    const epoch = this.channelEpoch;
    const stale = () => epoch !== this.channelEpoch;
    const run = async (): Promise<CheckOutcome> => {
      let started = false;
      try {
        if (await this.usesStoreUpdates()) {
          if (stale()) return { kind: "stale" };
          this.setStatus({ result: "store" });
          return { kind: "store" };
        }
        if (background) {
          const lastCheck = localStorage.getItem(LAST_UPDATE_CHECK_KEY);
          if (lastCheck && Date.now() - parseInt(lastCheck, 10) < UPDATE_CHECK_INTERVAL) {
            log.debug("Skipping update check - too soon since last check");
            return { kind: "skipped" };
          }
        }
        started = true;
        this.setStatus({ checking: true });
        const update = await invoke<AppUpdateInfo | null>("check_for_app_update");
        if (stale()) return { kind: "stale" };
        const now = Date.now();
        localStorage.setItem(LAST_UPDATE_CHECK_KEY, now.toString());
        // Null results are published too, so a stale offer disappears. A restart already pending
        // keeps the install state; the offer stays empty until the app restarts.
        this.setStatus({
          available: this.status.install === "restart_pending" ? null : update,
          lastChecked: now,
          result: "current",
        });
        return update ? { kind: "update", update } : { kind: "current" };
      } catch (error) {
        if (stale()) return { kind: "stale" };
        log.error("Update check failed:", error);
        this.setStatus({ result: "error" });
        return { kind: "error" };
      } finally {
        if (!stale()) {
          this.pendingCheck = null;
          if (started) this.setStatus({ checking: false });
        }
      }
    };
    const promise = run();
    this.pendingCheck = promise;
    return promise;
  }

  /** Quiet check for the status UI: no toasts and no dialog. Resolves to the status after the check ends. */
  async checkNow(): Promise<UpdateStatus> {
    let outcome = await this.runCheck(false);
    // A background check that skipped itself has not checked anything; run a real one.
    if (outcome.kind === "skipped") outcome = await this.runCheck(false);
    return this.status;
  }

  /**
   * The update channel changed. The offer, the last result and any check started under the old
   * channel no longer apply; callers start a fresh check afterwards.
   */
  notifyChannelChanged(channel: UpdateChannel): void {
    if (this.channel === channel) return;
    this.channel = channel;
    this.channelEpoch += 1;
    this.pendingCheck = null;
    this.setStatus({ checking: false, available: null, lastChecked: null, result: "idle" });
  }

  /** Opens the existing install dialog for the update found by the last check. */
  async promptInstall(): Promise<void> {
    const update = this.status.available;
    if (update) await this.beginInstall(update);
  }

  /** Restarts into an installed update (defers while a recording is active). */
  async restartNow(): Promise<void> {
    if (this.status.install !== "restart_pending") return;
    await this.performRelaunch();
  }

  /**
   * Set whether user is in an active session (recording/transcribing)
   * Auto-updates are skipped when session is active
   * When session ends, pending relaunch will be triggered
   */
  setSessionActive(active: boolean): void {
    this.isSessionActive = active;

    // If session ended and we have a pending relaunch, do it now
    if (!active && this.pendingRelaunch) {
      this.pendingRelaunch = false;
      this.performRelaunch();
    }
  }

  /**
   * Perform relaunch with backend verification and error handling
   */
  private async performRelaunch(): Promise<void> {
    if (await this.usesStoreUpdates()) {
      return;
    }

    // Final safety check: verify with backend that no session is active
    try {
      const currentState = await invoke<{ state: string }>("get_current_recording_state");
      const isActive = currentState.state !== "idle" && currentState.state !== "error";
      if (isActive) {
        log.debug("Backend still in active session, deferring relaunch");
        this.pendingRelaunch = true;
        return;
      }
    } catch (error) {
      log.error("Failed to check recording state, proceeding with relaunch:", error);
    }

    // Store the version we're updating to so the next launch can show it
    if (this.pendingUpdateVersion) {
      localStorage.setItem(JUST_UPDATED_KEY, this.pendingUpdateVersion);
    }

    try {
      await relaunch();
    } catch (error) {
      log.error("Relaunch failed:", error);
      toast.error("Update installed. Please restart the app manually.", {
        duration: 10000,
      });
    }
  }

  /**
   * Read and clear the just-updated version marker (one-shot).
   * Returns the version string if the app was just updated, then clears it.
   * Returns null if no marker exists.
   */
  getJustUpdatedVersion(): string | null {
    if (isStoreDistribution(this.distributionInfo)) {
      localStorage.removeItem(JUST_UPDATED_KEY);
      return null;
    }

    const version = localStorage.getItem(JUST_UPDATED_KEY);
    if (version === this.pendingUpdateVersion) {
      return null;
    }
    if (version) {
      localStorage.removeItem(JUST_UPDATED_KEY);
    }
    return version;
  }

  static getInstance(): UpdateService {
    if (!UpdateService.instance) {
      UpdateService.instance = new UpdateService();
    }
    return UpdateService.instance;
  }

  private async getDistributionInfo(): Promise<DistributionInfo> {
    if (this.distributionInfo) {
      return this.distributionInfo;
    }

    if (!this.distributionInfoPromise) {
      this.distributionInfoPromise = invoke<DistributionInfo>("get_distribution_info")
        .then((info) => {
          this.distributionInfo = info;
          return info;
        })
        .catch((error) => {
          console.error("Failed to read distribution info, assuming direct install:", error);
          const fallback: DistributionInfo = {
            channel: "direct",
            is_store_install: false,
            package_family_name: null,
          };
          this.distributionInfo = fallback;
          return fallback;
        });
    }

    return this.distributionInfoPromise;
  }

  private async usesStoreUpdates(): Promise<boolean> {
    return isStoreDistribution(await this.getDistributionInfo());
  }

  /**
   * Initialize the update service
   * - Checks for updates on startup if enabled
   * - Sets up daily update checks
   */
  async initialize(settings: AppSettings): Promise<void> {
    this.channel ??= settings.update_channel ?? "stable";
    if (await this.usesStoreUpdates()) {
      return;
    }
    // Check if automatic update checks are enabled (default to true if not set)
    const autoUpdateEnabled = settings.check_updates_automatically ?? true;

    if (!autoUpdateEnabled) {
      log.debug("Automatic update checks are disabled");
      return;
    }

    // Check on startup and then daily. Background checks notify; they do not install.
    await this.checkForUpdatesInBackground();

    this.setupDailyUpdateCheck();
  }

  /**
   * Request notification permission (call after onboarding)
   */
  async requestNotificationPermission(): Promise<boolean> {
    if (await this.usesStoreUpdates()) {
      return false;
    }

    try {
      let permitted = await isPermissionGranted();
      if (!permitted) {
        const permission = await requestPermission();
        permitted = permission === "granted";
      }
      // Send a welcome notification so user sees what they're allowing
      if (permitted) {
        sendNotification({
          title: "Notifications Enabled",
          body: "You'll be notified about app updates",
        });
      }
      return permitted;
    } catch (error) {
      log.error("Failed to request notification permission:", error);
      return false;
    }
  }

  /**
   * Send a system notification (macOS notification center)
   */
  private async sendSystemNotification(title: string, body: string): Promise<void> {
    try {
      let permitted = await isPermissionGranted();
      if (!permitted) {
        const permission = await requestPermission();
        permitted = permission === "granted";
      }
      if (permitted) {
        sendNotification({ title, body });
      }
    } catch (error) {
      log.error("Failed to send system notification:", error);
    }
  }

  /**
   * Check for updates in the background.
   * Background checks notify the user; they never download or install automatically.
   * Runs on startup and daily when automatic update checks are enabled.
   */
  async checkForUpdatesInBackground(): Promise<void> {
    const outcome = await this.runCheck(true);
    if (outcome.kind === "update") await this.handleUpdateAvailable(outcome.update, true);
    // Background failures stay quiet: the status shows them, no toast.
  }

  /**
   * Check for updates with user feedback (manual check)
   */
  async checkForUpdatesManually(): Promise<void> {
    if (await this.usesStoreUpdates()) {
      toast.info("Updates are handled by Microsoft Store");
      return;
    }
    toast.info("Checking for updates...");
    let outcome = await this.runCheck(false);
    if (outcome.kind === "skipped") outcome = await this.runCheck(false);
    switch (outcome.kind) {
      case "update":
        await this.handleUpdateAvailable(outcome.update, false);
        break;
      case "current":
        toast.success("You're on the latest version!");
        break;
      case "error":
        toast.error("Failed to check for updates");
        break;
      default:
        break;
    }
  }

  /**
   * Handle when an update is available
   */
  private async handleUpdateAvailable(
    update: AppUpdateInfo,
    isBackgroundCheck: boolean,
  ): Promise<void> {
    if (this.status.install === "restart_pending") return;
    if (isBackgroundCheck) {
      toast.info(`Update ${update.version} is available. Open Settings to install it.`);
      await this.sendSystemNotification(
        "Update Available",
        `Voicetypr ${update.version} is ready to install from Settings.`,
      );
      return;
    }

    await this.beginInstall(update);
  }

  /**
   * Show update dialog and handle user response. Only one prompt or install runs at a time; a
   * second call while one is open, downloading or waiting for a restart does nothing.
   */
  private async beginInstall(update: AppUpdateInfo): Promise<void> {
    if (this.status.install !== "idle") return;
    this.setStatus({ install: "prompting" });
    try {
      // Native three-button messages work even when the main window is hidden in the tray.
      // Manifest notes belong only in What's new, never in an install confirmation.
      // rfd's macOS backend has no cancel-key option: AppKit binds Escape by this title.
      const choice = await message(
        `Voicetypr ${update.version} is available.\n\nDownload and install it now? The app will restart when recording ends.`,
        {
          title: "Update Available",
          kind: "info",
          buttons: { yes: "Update", no: "View notes", cancel: "Cancel" },
        },
      );
      if (choice === "View notes") {
        await openReleaseLink(releasePageUrl(update.version));
      }
      if (choice !== "Update") {
        this.setStatus({ install: "idle" });
        return;
      }
    } catch (error) {
      log.error("Update dialog failed:", error);
      this.setStatus({ install: "idle" });
      return;
    }

    this.setStatus({ install: "installing" });
    toast.info("Downloading update...");
    try {
      await invoke("install_app_update", {
        expectedVersion: update.version,
      });
    } catch (error) {
      log.error("Update installation failed:", error);
      toast.error("Failed to install update");
      this.setStatus({ install: "idle" });
      return;
    }
    // Installed: the offer is replaced by "restart pending" until the app restarts.
    this.pendingUpdateVersion = update.version;
    this.setStatus({ install: "restart_pending", restartVersion: update.version, available: null });
    // Notify if relaunch will be deferred due to active session
    if (this.isSessionActive) {
      await this.sendSystemNotification(
        "Update Ready",
        "Voicetypr will restart when recording ends",
      );
    }
    await this.performRelaunch();
  }

  /**
   * Set up daily update checks
   */
  private setupDailyUpdateCheck(): void {
    // Clear any existing timer
    if (this.updateCheckTimer) {
      clearInterval(this.updateCheckTimer);
    }

    // Set up interval for daily checks
    this.updateCheckTimer = window.setInterval(() => {
      this.checkForUpdatesInBackground();
    }, UPDATE_CHECK_INTERVAL);

    log.debug("Daily update check scheduled");
  }

  /**
   * Clean up resources
   */
  dispose(): void {
    if (this.updateCheckTimer) {
      clearInterval(this.updateCheckTimer);
      this.updateCheckTimer = null;
    }
  }
}

// Export singleton instance
export const updateService = UpdateService.getInstance();
