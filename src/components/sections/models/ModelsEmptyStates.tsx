import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Spinner } from "@/components/ui/spinner";
import { Bot } from "lucide-react";

interface ModelsEmptyStatesProps {
  isLoading: boolean;
  hasModels: boolean;
  hasRemoteServers: boolean;
}

export function ModelsEmptyStates({
  isLoading,
  hasModels,
  hasRemoteServers,
}: ModelsEmptyStatesProps) {
  if (isLoading && !hasModels) {
    return (
      <Empty className="border border-border/60 bg-card/70 py-12">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <Spinner className="size-5" />
          </EmptyMedia>
          <EmptyTitle>Loading models</EmptyTitle>
          <EmptyDescription>Checking available transcription sources.</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  if (!isLoading && !hasModels && !hasRemoteServers) {
    return (
      <Empty className="border border-border/60 bg-card/70 py-12">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <Bot className="size-5" />
          </EmptyMedia>
          <EmptyTitle>No models available</EmptyTitle>
          <EmptyDescription>Models will appear here when they become available.</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  return null;
}
