import type { ReactNode } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { DictionaryKind } from "@/lib/dictionary-validation";

const kinds: DictionaryKind[] = ["words", "corrections", "snippets"];

export function DictionaryTabs({
  value,
  onValueChange,
  counts,
  children,
  toolbar,
}: {
  value: DictionaryKind;
  onValueChange: (value: DictionaryKind) => void;
  counts: Record<DictionaryKind, number>;
  children: ReactNode;
  toolbar?: ReactNode;
}) {
  return (
    <Tabs
      className="gap-[18px]"
      value={value}
      onValueChange={(next) => onValueChange(next as DictionaryKind)}
    >
      <div data-pencil-name="Toolbar" className="flex flex-wrap items-center gap-2.5">
        <TabsList
          aria-label="Dictionary tabs"
          activateOnFocus
          className="h-auto w-fit max-w-full gap-0.5 overflow-x-auto rounded-[10px] bg-muted p-[3px]"
        >
          {kinds.map((kind) => (
            <TabsTrigger
              key={kind}
              value={kind}
              className="h-auto rounded-[7px] px-3 py-1.5 text-[12.5px] leading-[normal] data-active:bg-card dark:data-active:bg-card dark:data-active:border-transparent"
            >
              {kind[0].toUpperCase() + kind.slice(1)} {counts[kind]}
            </TabsTrigger>
          ))}
        </TabsList>
        {toolbar}
      </div>
      {kinds.map((kind) => (
        <TabsContent key={kind} value={kind}>
          {value === kind ? children : null}
        </TabsContent>
      ))}
    </Tabs>
  );
}
