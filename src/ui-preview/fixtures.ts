import type { Step } from "@/components/onboarding/onboardingTypes";
import { createUsageFixture } from "@/ui-preview/usageFixture";
import { DEFAULT_PILL_INDICATOR_MODE, type AppSettings, type BuiltinPolishAvailability, type LicenseStatus, type ModelInfo, type PolishStatus, type TranscriptionHistory } from "@/types";
import type { WritingSettings } from "@/types/writing";
import type { ShortcutActionDefinition, ShortcutSettings } from "@/types/shortcuts";
import type { AiProvider, AIProviderModel } from "@/types/providers";
import type { AISettings, EnhancementOptions } from "@/types/ai";
import type { AccelerationStatus } from "@/types/acceleration";
import type { DistributionInfo } from "@/types/distribution";
import type { SharingStatus, FirewallStatus } from "@/components/sections/network-sharing/types";
import type { EffectivePrimaryShortcut } from "@/lib/primary-shortcut";

export type PreviewOptions = {
  theme: "light" | "dark";
  platform: "macos" | "windows";
  empty: boolean;
  sidebar?: "expanded" | "rail";
  onboarding?: Step;
  /** `?case=`: an onboarding edge case, such as a denied microphone. */
  onboardingCase?: OnboardingPreviewCase;
  /** `?license=none`: a device whose free trial has not been started. */
  trialNotStarted?: boolean;
  /**
   * `?polish=`: built-in Polish switched `off`, or what Voicetypr's server says about this device
   * (`no_access`, `not_entitled`, `unreachable`). Default: on and confirmed.
   */
  polish?: "off" | BuiltinPolishAvailability;
};

const demoText = [
  "Let's move the sync to Thursday at 3 and I'll send the notes before.",
  "Refactor the stream tap so the sink is created after play.",
  "Thanks for the quick turnaround — the new build fixed it.",
  "Buy oat milk, coffee filters and the birthday card for Sam.",
  "Can you review the pricing page copy before lunch?",
  "Please move the design review to Thursday afternoon and send the updated agenda to the team.",
  "The new onboarding flow feels much clearer. I would shorten the first explanation and make the action easier to spot.",
  "Could you share the latest project estimate before our call tomorrow? I want to review the scope with the client.",
  "Today's priorities are to review the release notes, test the recording shortcut, and check the new settings layout. Then collect the feedback from the team, compare the Mac and Windows flows, document any differences we can reproduce, and send one clear summary with screenshots and next steps before the end of the day.",
  "I had an idea for the introduction: show the finished result first, then explain how the workflow gets there.",
  "The bug appears after switching from the shortcuts pane back to general settings. The previous pane remains visible.",
  "Thanks for the thoughtful feedback. I'll update the draft and share a revised version later today.",
  "Research note: compare local transcription speed with the cloud option using the same real speech sample.",
  "Let's keep the interface focused on the task and move the advanced controls into a separate settings section.",
  "The presentation is ready for review. Please check the opening slide and the final call to action.",
];

type HistoryWireItem = Omit<TranscriptionHistory, "id" | "timestamp"> & { timestamp: string };
const apps = [
  "Slack",
  "Cursor",
  "Mail",
  "Notes",
  "Slack",
  "Pages",
  "Mail",
  "Cursor",
  "Notes",
  "Chrome",
  "Linear",
  "Pages",
  "Mail",
  "Notes",
  "Slack",
];
export const demoHistory: HistoryWireItem[] = demoText.map((text, index) => ({
  timestamp: new Date(
    Date.now() -
      (index < 5
        ? [2, 26, 73, 159, 172][index] * 60 * 1000
        : index < 10
          ? 24 * 60 * 60 * 1000 + (index - 5) * 60 * 60 * 1000
          : 48 * 60 * 60 * 1000 + (index - 10) * 60 * 60 * 1000),
  ).toISOString(),
  text,
  model: index === 7 || index === 12 ? "small.en" : "parakeet-tdt-0.6b-v3",
  status: "completed",
  writing: {
    source: index === 7 || index === 12 ? "audio_file" : "desktop_recording",
    engine: index === 7 || index === 12 ? "whisper" : "parakeet",
    audio_duration_ms: 4100 + index * 380,
    ...(index === 7 || index === 12 ? {} : { context_hint: { app_name: apps[index] } }),
    ...(index === 0 || index === 5 || index === 10
      ? {
          ai_applied: true,
          mode: "Message",
          original_text:
            index === 0
              ? "so um let's move the sync to uh thursday at three and I'll send the notes before"
              : `um ${text.toLowerCase()}`,
        }
      : {}),
  },
}));

export const demoDownloadProgress = {
  model: "large-v3-turbo",
  engine: "whisper",
  downloaded: 241_000_000,
  total: 574_000_000,
  progress: 42,
  phase: "downloading",
};

export const demoModels: ModelInfo[] = [
  ...([
    ["parakeet-tdt-0.6b-v3", "Parakeet V3", "parakeet", true, true, 500_000_000, 9, 9],
    ["parakeet-tdt-0.6b-v2", "Parakeet V2 (English)", "parakeet", false, false, 480_000_000, 9, 8],
    ["parakeet-unified-640ms", "Parakeet Unified (English)", "parakeet", false, false, 620_000_000, 9, 10],
    ["nemotron-multilingual-1120ms", "Nemotron 3.5", "parakeet", false, false, 665_000_000, 8, 9],
    ["base.en", "Base (English)", "whisper", false, true, 147_964_211, 8, 5],
    ["small.en", "Small (English)", "whisper", false, false, 487_614_201, 7, 6],
    ["large-v3-turbo", "Large v3 Turbo", "whisper", false, false, 1_624_555_275, 7, 9],
    ["large-v3", "Large v3", "whisper", false, false, 3_095_033_483, 2, 9],
  ] as const).map(([name, display_name, engine, recommended, downloaded, size, speed_score, accuracy_score]): ModelInfo => ({
    name,
    display_name,
    engine,
    kind: "local",
    recommended,
    downloaded,
    requires_setup: false,
    size,
    url: "",
    sha256: "",
    speed_score,
    accuracy_score,
  })),
  ...(["soniox", "deepgram", "openai", "groq", "cohere"] as const).map((engine): ModelInfo => ({
    name: engine,
    display_name: engine[0].toUpperCase() + engine.slice(1),
    engine,
    kind: "cloud",
    recommended: false,
    downloaded: engine === "soniox",
    requires_setup: engine !== "soniox",
  })),
];

export const demoWriting: WritingSettings = {
  custom_words: [
    { phrase: "Voicetypr", spoken_form: "voice typer", enabled: true },
    { phrase: "Parakeet", spoken_form: "para keet", enabled: true },
    { phrase: "Ideaplexa", enabled: true },
  ],
  replacements: [
    { from: "voice typer", to: "Voicetypr", enabled: true },
    { from: "next js", to: "Next.js", enabled: true },
    { from: "post hog", to: "PostHog", enabled: true },
  ],
  snippets: [
    {
      trigger: "intro",
      body: "Hi, thanks for getting in touch. I'll take a look and follow up shortly.",
      enabled: true,
      preserve_literal: false,
    },
    {
      trigger: "insert my email",
      body: "hello@example.com",
      enabled: true,
      preserve_literal: true,
    },
  ],
  app_formatting_rules: [
    { app_name: "Slack", preset: "Message", enabled: true },
    { app_name: "Cursor", preset: "Code", enabled: true },
  ],
  spoken_punctuation: false,
  editing_commands: false,
};

const shortcutActions: ShortcutActionDefinition[] = [
  {
    action: "toggle_recording",
    label: "Toggle recording",
    description: "Start or stop dictation.",
    section: "Recording",
    recommended_trigger: "pressed",
    allows_single_key: false,
  },
  {
    action: "hold_to_record",
    label: "Hold to record",
    description: "Record while holding the shortcut.",
    section: "Recording",
    recommended_trigger: "hold",
    allows_single_key: false,
  },
  {
    action: "cancel_recording",
    label: "Cancel dictation",
    description: "Discard the current recording.",
    section: "Recording",
    recommended_trigger: "pressed",
    allows_single_key: false,
  },
  {
    action: "copy_last_transcription",
    label: "Copy last transcript",
    description: "Copy the most recent text.",
    section: "History",
    recommended_trigger: "pressed",
    allows_single_key: false,
  },
  {
    action: "paste_last_transcription",
    label: "Paste last transcript",
    description: "Paste the most recent text.",
    section: "History",
    recommended_trigger: "pressed",
    allows_single_key: false,
  },
  {
    action: "toggle_ai_formatting",
    label: "Polish on / off",
    description: "Enable or disable Polish.",
    section: "Polish",
    recommended_trigger: "pressed",
    allows_single_key: false,
  },
  {
    action: "open_dashboard",
    label: "Open Voicetypr",
    description: "Show the main window.",
    section: "App",
    recommended_trigger: "pressed",
    allows_single_key: false,
  },
];

export type OnboardingPreviewCase = "heard" | "polished" | "downloading" | "download-failed" | "mic-denied" | "ax-skipped" | "offline" | "key-error" | "expired" | "licensed" | "no-access" | "cloud" | "remote" | "mic-new" | "check-failed";

export function createFixtures(options: PreviewOptions) {
  const hotkey = options.platform === "macos" ? "Alt+Space" : "Control+Space";
  const settings: AppSettings = {
    hotkey,
    current_model: options.onboardingCase === "cloud" ? "soniox" : "parakeet-tdt-0.6b-v3",
    current_model_engine: options.onboardingCase === "cloud" ? "soniox" : "parakeet",
    speech_language: "en",
    theme: options.theme,
    onboarding_completed: !options.onboarding,
    builtin_polish_enabled: options.polish !== "off",
    check_updates_automatically: false,
    recording_mode: options.onboarding ? "toggle" : "push_to_talk",
    selected_microphone:
      options.platform === "macos" ? "MacBook Pro Microphone" : "Built-in Microphone",
    auto_paste_transcription: true,
    keep_transcription_in_clipboard: false,
    play_sound_on_recording: true,
    play_sound_on_transcription_complete: true,
    pill_indicator_mode: DEFAULT_PILL_INDICATOR_MODE,
    pill_indicator_style: "full",
    pill_indicator_position: "bottom-center",
    transcription_mode: "live_preview",
    update_channel: "beta",
    save_recordings: true,
  };
  const downloadCase = options.onboardingCase === "downloading" || options.onboardingCase === "download-failed";
  const onboardingModels = options.onboarding
    ? demoModels.map((model) => {
        if (model.name !== "parakeet-tdt-0.6b-v3") return model;
        return options.platform === "macos"
          ? {
              ...model,
              display_name: "Parakeet V3",
              size: 500_000_000,
              downloaded: options.onboarding !== "promise" && !downloadCase,
            }
          : {
              ...model,
              display_name: "Parakeet V3",
              size: 670_479_942,
              downloaded: options.onboarding !== "promise" && !downloadCase,
            };
      })
    : demoModels;
  const beforeTrial = options.onboarding !== "done" || options.onboardingCase === "no-access";
  const license: LicenseStatus = options.onboardingCase === "expired"
    ? { status: "expired" }
    : options.trialNotStarted || (options.onboarding && beforeTrial)
    ? { status: "none" }
    : options.onboarding && options.onboardingCase !== "licensed"
    ? { status: "trial", trial_days_left: 7, expires_at: "2026-11-24T12:00:00Z" }
    : {
        status: "licensed",
        license_type: "pro",
        trial_days_left: 0,
        verification_state: "verified",
      };
  const shortcuts: ShortcutSettings = {
    bindings: [
      {
        id: "preview-record",
        action: "toggle_recording",
        shortcut: hotkey,
        trigger: "pressed",
        enabled: true,
        allow_risky_combo: false,
      },
    ],
  };
  const effectivePrimaryShortcut: EffectivePrimaryShortcut = {
    hotkey,
    binding: null,
    mode: "hold",
  };
  const providers: AiProvider[] = [
    { id: "openai", name: "OpenAI", status: "production", requires_api_key: true },
    { id: "anthropic", name: "Anthropic", status: "production", requires_api_key: true },
    { id: "gemini", name: "Gemini", status: "production", requires_api_key: true },
  ];
  const aiSettings: AISettings = {
    enabled: !options.onboarding,
    provider: "openai",
    model: "gpt-6-luna",
    hasApiKey: true,
    modelsByProvider: { openai: "gpt-6-luna" },
    reasoningByProvider: {},
    fastModeByProvider: {},
  };
  const enhancement: EnhancementOptions = { preset: "CleanDictation" };
  const providerModels: AIProviderModel[] = [
    { id: "gpt-6-luna", name: "GPT-6 Luna", recommended: true },
  ];
  const acceleration: AccelerationStatus = {
    mode: "auto",
    effective_backend: "cpu",
    gpu_available: false,
    message: "CPU transcription is ready.",
    diagnostic_code: "ready",
    recommended_action: "none",
  };
  const distribution: DistributionInfo = {
    channel: "direct",
    is_store_install: false,
    package_family_name: null,
  };
  const sharing: SharingStatus = {
    enabled: true,
    port: 4894,
    model_name: "Parakeet v3",
    server_name: "Demo Mac",
    active_connections: 1,
    password_configured: true,
    binding_results: [{ ip: "192.168.1.20", success: true, error: null, interface_name: "Wi-Fi" }],
    allow_model_control: false,
  };
  const firewall: FirewallStatus = {
    firewall_enabled: false,
    app_allowed: true,
    may_be_blocked: false,
  };
  const history = options.empty ? [] : demoHistory;
  const writing = options.empty
    ? { ...demoWriting, custom_words: [], replacements: [], snippets: [] }
    : demoWriting;
  const responses: Record<string, unknown> = {
    "plugin:app|version": "2.1.0-beta.3",
    "plugin:log|log": null,
    get_settings: settings,
    list_kept_dictations: [],
    get_microphone_permission_status: options.onboardingCase === "mic-denied" ? "denied"
      : options.onboardingCase === "mic-new" ? "not_determined"
      : options.onboarding === "promise" || options.onboarding === "try" ? "not_determined" : "granted",
    start_microphone_test: "preview",
    stop_microphone_test: null,
    save_settings: null,
    check_license_status: license,
    revalidate_license: license,
    start_trial: { status: "trial", trial_days_left: 7, expires_at: "2026-11-24T12:00:00Z" } satisfies LicenseStatus,
    get_model_status: { models: onboardingModels },
    get_recognition_availability_snapshot: {
      whisper_available: false,
      parakeet_available: true,
      cloud_selected: false,
      cloud_ready: false,
      remote_selected: false,
      remote_available: false,
    },
    get_transcription_history: history,
    get_usage_stats: createUsageFixture(null, options.empty),
    get_transcription_count: history.length,
    get_writing_settings: writing,
    update_writing_settings: null,
    list_dictionary_suggestions: options.empty
      ? []
      : [{ id: "preview-1", heard: "cloud code", word: "Claude Code", status: "pending" }],
    set_dictionary_suggestion_status: null,
    update_transcription_text: [
      { id: "preview-2", heard: "voice typer", word: "Voicetypr", status: "pending" },
    ],
    get_shortcut_settings: shortcuts,
    get_effective_primary_shortcut: effectivePrimaryShortcut,
    list_shortcut_actions: shortcutActions,
    get_ai_settings: aiSettings,
    polish_status: {
      will_run: !options.onboarding || !options.polish,
      own_provider_ready: !options.onboarding,
      own_provider_enabled: !options.onboarding,
      builtin_enabled: options.polish !== "off",
      builtin: !options.onboarding ? "own_provider" : options.polish && options.polish !== "off" ? options.polish : "ready",
    } satisfies PolishStatus,
    polish_can_run: !options.onboarding || !options.polish,
    get_ai_settings_for_provider: aiSettings,
    list_ai_providers: providers,
    get_enhancement_options: enhancement,
    get_openai_config: { baseUrl: "https://api.openai.com/v1" },
    list_provider_models: providerModels,
    get_audio_devices: [
      options.platform === "macos" ? "MacBook Pro Microphone" : "Built-in Microphone",
      "USB Microphone",
    ],
    validate_microphone_selection: false,
    check_microphone_permission: true,
    check_accessibility_permission: !(options.onboarding === "permissions" || options.onboardingCase === "ax-skipped"),
    get_tray_status: { available: true, attempts: 0, lastError: null },
    get_distribution_info: distribution,
    check_for_app_update: null,
    get_telemetry_status: { enabled: true, available: true },
    get_product_analytics_status: { enabled: true, available: true, consent_required: false },
    get_autostart_status: false,
    get_current_recording_state: { state: "idle" },
    get_onboarding_try: { kind: "tries", left: 10 },
    start_onboarding_try: { id: "preview", left: 9 },
    stop_onboarding_try: {
      heard: "um so let's move the the sync to thursday no wait friday at three and uh i'll send the notes before",
      cleaned: options.polish === "off" ? null : "Let’s move the sync to Friday at 3, and I’ll send the notes before.",
    },
    arm_onboarding_try: null,
    cli_tool_status: {
      installed: true,
      manageable: true,
      path:
        options.platform === "macos"
          ? "/usr/local/bin/voicetypr"
          : "C:\\Program Files\\Voicetypr\\voicetypr.exe",
      app_version: "2.1.0-beta.3",
      command_version: "2.1.0-beta.3",
      compatible: true,
      detail: null,
    },
    list_remote_servers: options.onboardingCase === "remote"
      ? [{ id: "studio", name: "Studio Mac mini", host: "192.168.1.20", port: 47842, status: "Online", model: "parakeet-tdt-0.6b-v3", created_at: 0 }]
      : [],
    discover_remote_servers: [],
    get_active_remote_server: options.onboardingCase === "remote" ? "studio" : null,
    check_remote_server_status: { id: "studio", name: "Studio Mac mini", host: "192.168.1.20", port: 47842, status: "Online", model: "parakeet-tdt-0.6b-v3", created_at: 0 },
    get_sharing_status: sharing,
    get_local_ips: ["192.168.1.20"],
    get_firewall_status: firewall,
    get_soniox_storage_counts: { filesTotal: 0, transcriptionsTotal: 0 },
    get_active_stream_capabilities: {
      active_engine: "parakeet",
      capabilities: {
        supports_streaming: false,
        supports_committed_prefix: false,
        supports_tentative_tail: false,
        supports_endpointing: false,
        final_only: true,
      },
      eou_model_downloaded: false,
      eou_chunk_ms: 0,
      transcription_mode: "regular",
    },
    get_transcription_acceleration_status: acceleration,
    keyring_has: true,
    keyring_get: null,
    get_application_icon: null,
    get_recordings_directory:
      options.platform === "macos" ? "/Users/demo/Recordings" : "C:\\Users\\demo\\Recordings",
    check_recording_exists: false,
    get_local_machine_id: "preview-machine",
    get_device_id: "preview-device",
    get_system_specs: { os: options.platform, app_version: "2.1.0-beta.3" },
    get_latest_log_for_bug_report: { path: null, content: null },
  };
  return responses;
}
