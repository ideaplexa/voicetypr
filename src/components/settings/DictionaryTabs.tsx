import type { ReactNode } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { DictionaryKind } from "@/lib/dictionary-validation";

const kinds: DictionaryKind[] = ["words", "corrections", "snippets"];

export function DictionaryTabs({ value, onValueChange, counts, children }: {
  value: DictionaryKind;
  onValueChange: (value: DictionaryKind) => void;
  counts: Record<DictionaryKind, number>;
  children: ReactNode;
}) {
  return <Tabs value={value} onValueChange={(next) => onValueChange(next as DictionaryKind)}>
    <TabsList aria-label="Dictionary tabs" activateOnFocus className="w-fit max-w-full overflow-x-auto">
      {kinds.map((kind) => <TabsTrigger key={kind} value={kind}>
        {kind[0].toUpperCase() + kind.slice(1)} {counts[kind]}
      </TabsTrigger>)}
    </TabsList>
    {kinds.map((kind) => <TabsContent key={kind} value={kind}>
      {value === kind ? children : null}
    </TabsContent>)}
  </Tabs>;
}
