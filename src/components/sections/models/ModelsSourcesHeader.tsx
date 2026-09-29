import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { InfoButton, PageHeader } from "@/components/settings/settings-ui";

export function ModelsSourcesHeader() {
  return (
    <PageHeader
      title="Transcription"
      description="Choose where your voice is turned into text."
      action={
        <Dialog>
          <DialogTrigger render={<InfoButton label="Transcription guide" />} />
          <DialogContent className="sm:max-w-lg">
            <DialogHeader>
              <DialogTitle>Transcription guide</DialogTitle>
              <DialogDescription>
                Choose where speech recognition runs before recording or uploading files.
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-3 text-sm leading-6 text-muted-foreground">
              <p>
                <strong className="text-foreground">Local</strong> models run on this machine and
                keep raw audio local.
              </p>
              <p>
                <strong className="text-foreground">Cloud</strong> sources use a connected provider
                when you choose one.
              </p>
              <p>
                <strong className="text-foreground">Remote Voicetypr</strong> uses another device on
                your network when that server is online.
              </p>
            </div>
          </DialogContent>
        </Dialog>
      }
    />
  );
}
