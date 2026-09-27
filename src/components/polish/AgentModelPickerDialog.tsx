import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import { Check, ChevronDown, Search } from "lucide-react";
import type { AIProviderConfig } from "@/types/providers";
import { AGENT_CLI_DEFAULT_LABEL, fromModelSelectValue, type ModelPickerGroup } from "./agentCli";

export interface AgentModelPickerDialogProps {
  provider: AIProviderConfig;
  groups: ModelPickerGroup[];
  selectedItem: ModelPickerGroup["items"][number] | null;
  disabled: boolean;
  loading: boolean;
  onSelect: (modelId: string) => void;
  onOpen: () => void;
}

export function AgentModelPickerDialog({
  provider,
  groups,
  selectedItem,
  disabled,
  loading,
  onSelect,
  onOpen,
}: AgentModelPickerDialogProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const normalizedQuery = query.trim().toLowerCase();
  const visibleGroups = useMemo(
    () =>
      groups.flatMap((group) => {
        const items = normalizedQuery
          ? group.items.filter((item) =>
              [item.label, item.qualifiedId, item.model.sourceProvider ?? ""]
                .join(" ")
                .toLowerCase()
                .includes(normalizedQuery),
            )
          : group.items;
        return items.length > 0 ? [{ ...group, items }] : [];
      }),
    [groups, normalizedQuery],
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        setOpen(nextOpen);
        if (nextOpen) {
          onOpen();
        } else {
          setQuery("");
        }
      }}
    >
      <DialogTrigger
        render={
          <Button
            type="button"
            variant="outline"
            disabled={disabled}
            className="h-9 w-full justify-between gap-3 px-3 sm:w-52"
            aria-label={`Model for ${provider.name}`}
          />
        }
      >
        <span className="min-w-0 truncate text-left">
          {selectedItem?.label ?? AGENT_CLI_DEFAULT_LABEL}
        </span>
        {loading ? (
          <Spinner className="h-3.5 w-3.5 shrink-0" />
        ) : (
          <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-60" />
        )}
      </DialogTrigger>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Choose a {provider.name} model</DialogTitle>
          <DialogDescription>
            Search models reported by your installed CLI. The selection is saved for this agent.
          </DialogDescription>
        </DialogHeader>
        <div className="relative">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={`Search ${provider.name} models`}
            aria-label={`Search ${provider.name} models`}
            className="pl-9"
          />
        </div>
        <ScrollArea className="max-h-[min(60vh,28rem)] pr-3">
          <div className="space-y-5 py-1">
            {visibleGroups.length === 0 ? (
              <p className="rounded-lg border border-dashed px-4 py-8 text-center text-sm text-muted-foreground">
                No models match your search.
              </p>
            ) : (
              visibleGroups.map((group) => (
                <section key={`${provider.id}-${group.value}`} className="space-y-2">
                  <h3 className="px-1 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                    {group.value}
                  </h3>
                  <div className="grid gap-1">
                    {group.items.map((item) => {
                      const selected = selectedItem?.value === item.value;
                      const detail = [
                        item.model.reasoning ? "Reasoning" : "",
                        item.model.contextWindow
                          ? `${Math.round(item.model.contextWindow / 1000)}k context`
                          : "",
                      ]
                        .filter(Boolean)
                        .join(" · ");
                      return (
                        <button
                          key={item.value}
                          type="button"
                          aria-pressed={selected}
                          className={`flex min-h-14 w-full items-center gap-3 rounded-lg border px-3 py-2.5 text-left transition-colors ${
                            selected
                              ? "border-sage/50 bg-sage-bg/50"
                              : "border-transparent hover:border-border hover:bg-muted/50"
                          }`}
                          onClick={() => {
                            onSelect(fromModelSelectValue(item.value));
                            setOpen(false);
                          }}
                        >
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-sm font-medium">{item.label}</span>
                            <span className="mt-0.5 block truncate font-mono text-[11px] text-muted-foreground">
                              {item.qualifiedId}
                              {detail ? ` · ${detail}` : ""}
                            </span>
                          </span>
                          <Check
                            className={`h-4 w-4 shrink-0 text-sage ${
                              selected ? "opacity-100" : "opacity-0"
                            }`}
                          />
                        </button>
                      );
                    })}
                  </div>
                </section>
              ))
            )}
          </div>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}
