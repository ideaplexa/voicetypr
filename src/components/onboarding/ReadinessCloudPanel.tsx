import { EmptyState, LoadingState } from "@/components/onboarding/OnboardingChrome";
import { ApiKeyModal } from "@/components/ApiKeyModal";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { getCloudProviderByModel } from "@/lib/cloudProviders";
import { cn } from "@/lib/utils";
import type { ModelInfo } from "@/types";
import { Cloud } from "lucide-react";

export interface ReadinessCloudPanelProps {
  cloudModelNames: string[];
  models: Record<string, ModelInfo>;
  currentModel: string | undefined;
  isLoading: boolean;
  cloudModelSetup: string | null;
  isSavingCloudKey: boolean;
  isModelReady: (name: string) => boolean;
  onSelectCloud: (modelName: string) => void;
  onSetCloudModelSetup: (modelName: string | null) => void;
  onCloudKeySubmit: (apiKey: string) => void;
}

export function ReadinessCloudPanel({
  cloudModelNames,
  models,
  currentModel,
  isLoading,
  cloudModelSetup,
  isSavingCloudKey,
  isModelReady,
  onSelectCloud,
  onSetCloudModelSetup,
  onCloudKeySubmit,
}: ReadinessCloudPanelProps) {
  const activeCloudProvider = cloudModelSetup
    ? getCloudProviderByModel(cloudModelSetup)
    : undefined;

  return (
    <div className="flex flex-col gap-4">
      <Card className="rounded-2xl border border-border bg-card py-0 shadow-sm">
        <ScrollArea className="h-[320px]">
          <div className="flex flex-col gap-3 p-4">
            {cloudModelNames.map((name) => {
              const model = models[name];
              const provider = getCloudProviderByModel(name);
              if (!model || !provider) return null;
              const ready = isModelReady(name);
              const selected = currentModel === name;
              return (
                <Card
                  key={name}
                  size="sm"
                  className={cn(
                    "rounded-xl border border-border bg-muted/30",
                    selected && "border-sage/50 bg-sage-bg/40 ring-1 ring-sage/30",
                  )}
                >
                  <CardHeader>
                    <CardAction>
                      <Badge variant={ready ? "secondary" : "outline"}>
                        {ready ? "Connected" : "API key required"}
                      </Badge>
                    </CardAction>
                    <div className="flex items-start gap-3">
                      <div className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
                        <Cloud className="size-5" />
                      </div>
                      <div>
                        <CardTitle>{provider.displayName}</CardTitle>
                        <CardDescription>{provider.description}</CardDescription>
                      </div>
                    </div>
                  </CardHeader>
                  <CardFooter className="justify-end">
                    <Button
                      size="sm"
                      variant={selected ? "default" : "outline"}
                      onClick={() => {
                        if (ready) {
                          void onSelectCloud(name);
                        } else {
                          onSetCloudModelSetup(name);
                        }
                      }}
                    >
                      {selected ? "Selected" : ready ? "Use provider" : "Add API key"}
                    </Button>
                  </CardFooter>
                </Card>
              );
            })}
            {isLoading && cloudModelNames.length === 0 ? (
              <LoadingState label="Loading cloud providers" />
            ) : null}
            {!isLoading && cloudModelNames.length === 0 ? (
              <EmptyState
                title="No cloud providers available"
                description="Choose Local or Remote Voicetypr to continue."
              />
            ) : null}
          </div>
        </ScrollArea>
      </Card>
      {activeCloudProvider ? (
        <ApiKeyModal
          isOpen
          onClose={() => {
            if (!isSavingCloudKey) onSetCloudModelSetup(null);
          }}
          onSubmit={(apiKey) => void onCloudKeySubmit(apiKey)}
          providerName={activeCloudProvider.providerName}
          isLoading={isSavingCloudKey}
          description={`Enter your ${activeCloudProvider.providerName} API key. It is stored securely in the system keychain.`}
          docsUrl={activeCloudProvider.docsUrl}
        />
      ) : null}
    </div>
  );
}
