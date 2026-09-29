import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import { PageHeader, Segmented, SettingsPage } from "@/components/settings/settings-ui";
import { useWritingSettings } from "@/state/writingSettings";
import type { CustomWord, Snippet, TextReplacementRule, WritingSettings } from "@/types/writing";
import { Lightbulb, Pencil, Plus, Search, Trash2 } from "lucide-react";

type DictionaryTab = "words" | "corrections" | "snippets";
const emptyCopy: Record<DictionaryTab, string> = {
  words: "No words yet — add names, brands and jargon so Voicetypr spells them right.",
  corrections: "No corrections yet — add a phrase and its exact replacement.",
  snippets: "No snippets yet — save text to insert with a spoken trigger.",
};
const columns: Record<DictionaryTab, string[]> = {
  words: ["Word", "Sounds like", "Language", "Status"],
  corrections: ["From", "To", "Language", "Status"],
  snippets: ["Trigger", "Expansion", "Language", "Status"],
};

export function DictionarySection() {
  const settings = useWritingSettings((state) => state.settings);
  const loaded = useWritingSettings((state) => state.loaded);
  const load = useWritingSettings((state) => state.load);
  const update = useWritingSettings((state) => state.update);
  const [tab, setTab] = useState<DictionaryTab>("words");
  const [search, setSearch] = useState("");
  const [editing, setEditing] = useState<number | null>(null);

  useEffect(() => { void load(); }, [load]);
  const changeTab = (next: DictionaryTab) => { setTab(next); setSearch(""); setEditing(null); };
  const updateList = <K extends keyof WritingSettings>(key: K, items: WritingSettings[K]) => update({ [key]: items });
  const add = (kind: DictionaryTab) => {
    changeTab(kind);
    if (kind === "words") {
      setEditing(settings.custom_words.length);
      updateList("custom_words", [...settings.custom_words, { phrase: "", spoken_form: null, language: null, enabled: true }]);
    } else if (kind === "corrections") {
      setEditing(settings.replacements.length);
      updateList("replacements", [...settings.replacements, { from: "", to: "", language: null, enabled: true }]);
    } else {
      setEditing(settings.snippets.length);
      updateList("snippets", [...settings.snippets, { trigger: "", body: "", language: null, enabled: true, preserve_literal: true }]);
    }
  };
  const updateWord = (index: number, patch: Partial<CustomWord>) => updateList("custom_words", settings.custom_words.map((word, current) => current === index ? { ...word, ...patch } : word));
  const updateCorrection = (index: number, patch: Partial<TextReplacementRule>) => updateList("replacements", settings.replacements.map((rule, current) => current === index ? { ...rule, ...patch } : rule));
  const updateSnippet = (index: number, patch: Partial<Snippet>) => updateList("snippets", settings.snippets.map((snippet, current) => current === index ? { ...snippet, ...patch } : snippet));
  const remove = (index: number) => {
    if (tab === "words") updateList("custom_words", settings.custom_words.filter((_, current) => current !== index));
    if (tab === "corrections") updateList("replacements", settings.replacements.filter((_, current) => current !== index));
    if (tab === "snippets") updateList("snippets", settings.snippets.filter((_, current) => current !== index));
    setEditing(null);
  };
  const normalized = search.trim().toLocaleLowerCase();
  const visible = tab === "words"
    ? settings.custom_words.map((word, index) => ({ index, values: [word.phrase, word.spoken_form ?? "", word.language ?? "", word.enabled ? "On" : "Off"] }))
    : tab === "corrections"
      ? settings.replacements.map((rule, index) => ({ index, values: [rule.from, rule.to, rule.language ?? "", rule.enabled ? "On" : "Off"] }))
      : settings.snippets.map((snippet, index) => ({ index, values: [snippet.trigger, snippet.body, snippet.language ?? "", snippet.enabled ? "On" : "Off"] }));
  const filtered = visible.filter((row) => row.values.some((value) => value.toLocaleLowerCase().includes(normalized)));
  const label = tab === "words" ? "word" : tab === "corrections" ? "correction" : "snippet";

  return <SettingsPage wide className="pt-5">
    <PageHeader title="Dictionary" description="Teach Voicetypr your names and words. Works with every engine, with or without Polish."
      action={<Button disabled={!loaded} onClick={() => add("words")}><Plus className="size-4" /> Add word</Button>} />
    <div className="flex flex-wrap items-center gap-3">
      <div className="max-w-full overflow-x-auto"><Segmented label="Dictionary tabs" value={tab} onValueChange={(value) => changeTab(value as DictionaryTab)} options={[
        { value: "words", label: `Words ${settings.custom_words.length}` },
        { value: "corrections", label: `Corrections ${settings.replacements.length}` },
        { value: "snippets", label: `Snippets ${settings.snippets.length}` },
      ]} /></div>
      <div className="relative min-w-40 flex-1 sm:max-w-64">
        <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input aria-label={`Search ${tab}`} placeholder={`Search ${tab}`} value={search} onChange={(event) => { setSearch(event.target.value); setEditing(null); }} className="pl-9" />
      </div>
      {tab !== "words" ? <Button variant="outline" aria-label={tab === "corrections" ? "Add rule" : "Add saved text"} disabled={!loaded} onClick={() => add(tab)}><Plus className="size-4" /> Add {label}</Button> : null}
    </div>
    <section aria-label={tab === "words" ? "Dictionary" : tab === "corrections" ? "Corrections" : "Snippets"} className="overflow-hidden rounded-[14px] border border-border bg-card">
      <div className="overflow-x-auto">
        <table className="w-full min-w-[650px] table-fixed text-left">
          <thead className="bg-muted text-[11px] uppercase tracking-wide text-muted-foreground"><tr>
            {columns[tab].map((column) => <th key={column} scope="col" className="px-4 py-2.5 font-medium">{column}</th>)}
            <th scope="col" className="w-24 px-4 py-2.5 font-medium">Actions</th>
          </tr></thead>
          <tbody>{filtered.map(({ index, values }) => <Row key={`${tab}-${index}`} index={index} values={values}
            editing={editing === index} onEdit={() => setEditing(editing === index ? null : index)} onDelete={() => remove(index)} />)}</tbody>
        </table>
      </div>
      {filtered.length === 0 ? <div className="px-5 py-10 text-center text-sm text-muted-foreground">{normalized ? `No ${tab} match “${search}”.` : emptyCopy[tab]}</div> : null}
      {editing !== null && (tab === "words" ? settings.custom_words[editing] : tab === "corrections" ? settings.replacements[editing] : settings.snippets[editing]) ?
        <div className="border-t border-border bg-muted/40 p-4">
          {tab === "words" ? <WordFields item={settings.custom_words[editing]} index={editing} onChange={updateWord} /> : null}
          {tab === "corrections" ? <CorrectionFields item={settings.replacements[editing]} index={editing} onChange={updateCorrection} /> : null}
          {tab === "snippets" ? <SnippetFields item={settings.snippets[editing]} index={editing} onChange={updateSnippet} /> : null}
          <Button className="mt-3" size="sm" variant="outline" onClick={() => setEditing(null)}>Done</Button>
        </div> : null}
    </section>
    <div className="flex items-start gap-2 rounded-[10px] bg-sage-bg p-3.5 text-sm text-foreground">
      <Lightbulb className="mt-0.5 size-4 shrink-0 text-sage" aria-hidden="true" />
      {tab === "words" ? "Words correct spelling after transcription. Supported engines can also use them while recognizing speech." : tab === "corrections" ? "Corrections apply exact replacements after transcription, with or without Polish." : "Say “insert” followed by a snippet trigger to add saved text."}
    </div>
  </SettingsPage>;
}

function Row({ index, values, editing, onEdit, onDelete }: { index: number; values: string[]; editing: boolean; onEdit: () => void; onDelete: () => void }) {
  return <tr className="border-t border-border first:border-t-0">
    {values.map((value, column) => <td key={column} className={`truncate px-4 py-2.5 ${column === 0 ? "text-sm font-medium text-foreground" : "text-xs text-muted-foreground"}`} title={value}>{value || "—"}</td>)}
    <td className="px-2 py-1.5"><div className="flex items-center">
      <Button size="icon-sm" variant="ghost" aria-label={`Edit row ${index + 1}`} aria-expanded={editing} onClick={onEdit}><Pencil className="size-3.5" /></Button>
      <Button size="icon-sm" variant="ghost" aria-label={`Delete row ${index + 1}`} onClick={onDelete}><Trash2 className="size-3.5" /></Button>
    </div></td>
  </tr>;
}

function FieldInput({ label, value, onChange, placeholder }: { label: string; value: string; onChange: (value: string) => void; placeholder?: string }) {
  return <label className="min-w-0 flex-1 text-xs text-muted-foreground">{label}<Input className="mt-1 bg-card text-sm" value={value} placeholder={placeholder} onChange={(event) => onChange(event.target.value)} /></label>;
}
function EnabledField({ checked, onChange }: { checked: boolean; onChange: (value: boolean) => void }) {
  return <label className="flex items-center gap-2 self-end py-2 text-xs text-muted-foreground">Enabled <Switch checked={checked} onCheckedChange={onChange} /></label>;
}
function WordFields({ item, index, onChange }: { item: CustomWord; index: number; onChange: (index: number, patch: Partial<CustomWord>) => void }) {
  return <div className="flex flex-wrap items-end gap-3">
    <FieldInput label="Canonical" value={item.phrase} placeholder="Canonical phrase" onChange={(phrase) => onChange(index, { phrase })} />
    <FieldInput label="Spoken" value={item.spoken_form ?? ""} placeholder="Spoken form (optional)" onChange={(spoken_form) => onChange(index, { spoken_form: spoken_form || null })} />
    <FieldInput label="Language" value={item.language ?? ""} placeholder="Language code (optional, e.g. en)" onChange={(language) => onChange(index, { language: language || null })} />
    <EnabledField checked={item.enabled} onChange={(enabled) => onChange(index, { enabled })} />
  </div>;
}
function CorrectionFields({ item, index, onChange }: { item: TextReplacementRule; index: number; onChange: (index: number, patch: Partial<TextReplacementRule>) => void }) {
  return <div className="flex flex-wrap items-end gap-3">
    <FieldInput label="Match" value={item.from} placeholder="Text to match" onChange={(from) => onChange(index, { from })} />
    <FieldInput label="Replace" value={item.to} placeholder="Replacement text" onChange={(to) => onChange(index, { to })} />
    <FieldInput label="Language" value={item.language ?? ""} placeholder="Language code (optional, e.g. en)" onChange={(language) => onChange(index, { language: language || null })} />
    <EnabledField checked={item.enabled} onChange={(enabled) => onChange(index, { enabled })} />
  </div>;
}
function SnippetFields({ item, index, onChange }: { item: Snippet; index: number; onChange: (index: number, patch: Partial<Snippet>) => void }) {
  return <div className="space-y-3">
    <div className="flex flex-wrap items-end gap-3">
      <FieldInput label="Insert" value={item.trigger.replace(/^insert\s+/i, "")} placeholder="my signature" onChange={(value) => onChange(index, { trigger: value.trimStart() ? `insert ${value.trimStart()}` : "" })} />
      <label className="min-w-0 flex-[2] text-xs text-muted-foreground">Text to insert
        <Textarea className="mt-1 min-h-20 bg-card text-sm" value={item.body} placeholder="Saved text" onChange={(event) => onChange(index, { body: event.target.value })} />
      </label>
      <FieldInput label="Language" value={item.language ?? ""} placeholder="Language code (optional, e.g. en)" onChange={(language) => onChange(index, { language: language || null })} />
    </div>
    <div className="flex gap-4"><EnabledField checked={item.enabled} onChange={(enabled) => onChange(index, { enabled })} />
      <label className="flex items-center gap-2 text-xs text-muted-foreground">Keep text exact <Switch checked={item.preserve_literal} onCheckedChange={(preserve_literal) => onChange(index, { preserve_literal })} /></label></div>
  </div>;
}
