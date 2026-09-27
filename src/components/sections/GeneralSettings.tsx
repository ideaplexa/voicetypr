import { AppearanceCard } from "@/components/sections/general/AppearanceCard";
import { AppBehaviorCard } from "@/components/sections/general/AppBehaviorCard";
import { TelemetrySection } from "@/components/sections/TelemetrySection";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { useSettings } from "@/contexts/SettingsContext";
import { HelpCircle } from "lucide-react";

export function GeneralSettings() {
  const { settings, updateSettings } = useSettings();

  if (!settings) return null;

  return (
    <div className="h-full min-h-0 overflow-auto">
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-3.5 pb-4 pl-2 pr-4">
        <div className="mb-1 flex flex-wrap items-start gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">General</h1>
              <Dialog>
                <DialogTrigger
                  render={
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      aria-label="General guide"
                      className="size-7 rounded-full text-muted-foreground"
                    />
                  }
                >
                  <HelpCircle className="h-4 w-4" />
                </DialogTrigger>
                <DialogContent className="sm:max-w-lg">
                  <DialogHeader>
                    <DialogTitle>General guide</DialogTitle>
                    <DialogDescription>
                      Manage global appearance, startup, privacy, and update options.
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                    <p>
                      <strong className="text-foreground">Appearance</strong> controls the app
                      theme.
                    </p>
                    <p>
                      <strong className="text-foreground">App behavior</strong> controls
                      launch-at-login and update checks.
                    </p>
                    <p>
                      <strong className="text-foreground">Privacy</strong> controls anonymous
                      diagnostics and analytics.
                    </p>
                  </div>
                </DialogContent>
              </Dialog>
            </div>
            <p className="mt-0.5 text-sm text-muted-foreground">
              Manage appearance, startup, privacy, and app-wide options.
            </p>
          </div>
        </div>

        <AppearanceCard
          theme={settings.theme}
          onThemeChange={(theme) => void updateSettings({ theme })}
        />

        <AppBehaviorCard
          updateChannel={settings.update_channel}
          checkUpdatesAutomatically={settings.check_updates_automatically}
          onUpdateChannelChange={(channel) => updateSettings({ update_channel: channel })}
          onCheckUpdatesAutomaticallyChange={(checked) =>
            void updateSettings({ check_updates_automatically: checked })
          }
          onLaunchAtStartupResolved={(enabled) => updateSettings({ launch_at_startup: enabled })}
        />

        <TelemetrySection />
      </div>
    </div>
  );
}
