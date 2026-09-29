import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Search } from "lucide-react";

export interface RecentRecordingsFiltersProps {
  searchQuery: string;
  onSearchQueryChange: (value: string) => void;
  sourceFilter: string;
  onSourceFilterChange: (value: string) => void;
  dateFilter: string;
  onDateFilterChange: (value: string) => void;
  appFilter: string;
  onAppFilterChange: (value: string) => void;
  distinctAppNames: string[];
  resultCount: number;
  onClearFilters: () => void;
}

export function RecentRecordingsFilters({
  searchQuery,
  onSearchQueryChange,
  sourceFilter,
  onSourceFilterChange,
  dateFilter,
  onDateFilterChange,
  appFilter,
  onAppFilterChange,
  distinctAppNames,
  resultCount,
  onClearFilters,
}: RecentRecordingsFiltersProps) {
  const hasActiveFilters =
    sourceFilter !== "all" || appFilter !== "all" || dateFilter !== "all" || searchQuery;

  return (
    <div className="w-full">
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-0 flex-1">
          <Search className="absolute left-[10px] top-1/2 -translate-y-1/2 h-[14px] w-[14px] text-muted-foreground" />
          <input
            type="text"
            placeholder="Search your dictations…"
            aria-label="Search dictations"
            value={searchQuery}
            onChange={(e) => onSearchQueryChange(e.target.value)}
            className="h-[35px] w-full rounded-[10px] border border-border bg-card pl-8 pr-4 text-[13px] text-muted-foreground placeholder:text-text-3 focus:border-sage focus:outline-none"
          />
          {searchQuery && (
            <button
              type="button"
              aria-label="Clear search"
              onClick={() => onSearchQueryChange("")}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              ×
            </button>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Select
            items={[
              { value: "all", label: "All sources" },
              { value: "desktop_recording", label: "This device" },
              { value: "audio_file", label: "Upload" },
              { value: "remote_server", label: "Remote" },
              { value: "cli", label: "CLI" },
            ]}
            value={sourceFilter}
            onValueChange={(value) => value != null && onSourceFilterChange(value)}
          >
            <SelectTrigger
              aria-label="Source"
              className="h-[35px] w-auto min-w-[105px] gap-3 rounded-[10px] border-border bg-card px-[10px] text-[13px] text-muted-foreground"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All sources</SelectItem>
              <SelectItem value="desktop_recording">This device</SelectItem>
              <SelectItem value="audio_file">Upload</SelectItem>
              <SelectItem value="remote_server">Remote</SelectItem>
              <SelectItem value="cli">CLI</SelectItem>
            </SelectContent>
          </Select>
          <Select
            items={[
              { value: "all", label: "All time" },
              { value: "today", label: "Today" },
              { value: "last7", label: "Last 7 days" },
            ]}
            value={dateFilter}
            onValueChange={(value) => value != null && onDateFilterChange(value)}
          >
            <SelectTrigger
              aria-label="Time range"
              className="h-[35px] w-auto min-w-[124px] gap-3 rounded-[10px] border-border bg-card px-[10px] text-[13px] text-muted-foreground"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All time</SelectItem>
              <SelectItem value="today">Today</SelectItem>
              <SelectItem value="last7">Last 7 days</SelectItem>
            </SelectContent>
          </Select>
          {distinctAppNames.length > 0 && (
            <Select
              items={[
                { value: "all", label: "All apps" },
                ...distinctAppNames.map((name) => ({ value: name, label: name })),
              ]}
              value={appFilter}
              onValueChange={(value) => value != null && onAppFilterChange(value)}
            >
              <SelectTrigger
                aria-label="App"
                className="h-[35px] w-auto min-w-[105px] gap-3 rounded-[10px] border-border bg-card px-[10px] text-[13px] text-muted-foreground"
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All apps</SelectItem>
                {distinctAppNames.map((name) => (
                  <SelectItem key={name} value={name}>
                    {name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
          {hasActiveFilters && (
            <button
              type="button"
              onClick={onClearFilters}
              className="text-xs text-muted-foreground hover:text-foreground transition-colors"
            >
              Clear filters
            </button>
          )}
        </div>
      </div>
      {(searchQuery || sourceFilter !== "all" || appFilter !== "all" || dateFilter !== "all") && (
        <p className="text-xs text-muted-foreground">
          Found {resultCount} result{resultCount !== 1 ? "s" : ""}
        </p>
      )}
    </div>
  );
}
