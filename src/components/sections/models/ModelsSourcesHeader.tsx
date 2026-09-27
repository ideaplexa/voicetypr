import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { SettingRow, SettingsHeader } from "@/components/settings/settings-ui";
import { HelpCircle } from "lucide-react";
import type { ReactNode } from "react";

interface ModelsSourcesHeaderProps {
  currentSourceType: string;
  currentSourceLabel: string;
  children: ReactNode;
}

export function ModelsSourcesHeader({
  currentSourceType,
  currentSourceLabel,
  children,
}: ModelsSourcesHeaderProps) {
  return (
    <>
      <SettingsHeader
        title={
          <span className="inline-flex items-center gap-2">
            Sources
            <Dialog>
              <DialogTrigger
                render={
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="Sources guide"
                    className="size-7 rounded-full text-muted-foreground"
                  />
                }
              >
                <HelpCircle className="h-4 w-4" />
              </DialogTrigger>
              <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                  <DialogTitle>Sources guide</DialogTitle>
                  <DialogDescription>
                    Choose where speech recognition runs before recording or uploading files.
                  </DialogDescription>
                </DialogHeader>
                <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                  <p>
                    <strong className="text-foreground">Local</strong> models run on this machine
                    and keep raw audio local.
                  </p>
                  <p>
                    <strong className="text-foreground">Cloud</strong> sources use a connected
                    provider when you choose one.
                  </p>
                  <p>
                    <strong className="text-foreground">Remote Voicetypr</strong> uses another
                    device on your network when that server is online.
                  </p>
                </div>
              </DialogContent>
            </Dialog>
          </span>
        }
        description="Choose a local model, cloud provider, or remote Voicetypr server."
      />

      <section className="rounded-xl border border-border/80 bg-card p-5">
        <SettingRow
          className="!mt-0 !border-t-0 !pt-0"
          title="Current source"
          description="The transcription engine used for new recordings and uploads."
          control={
            <div className="flex max-w-full items-center justify-end gap-2">
              <Badge variant="secondary">{currentSourceType}</Badge>
              <span className="max-w-64 truncate text-sm font-medium text-foreground">
                {currentSourceLabel}
              </span>
            </div>
          }
        />
        {children}
      </section>
    </>
  );
}
