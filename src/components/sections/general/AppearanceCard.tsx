import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldTitle,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { normalizeTheme } from "@/hooks/useTheme";
import { Sun } from "lucide-react";

interface AppearanceCardProps {
  theme: string | null | undefined;
  onThemeChange: (theme: string) => void;
}

export function AppearanceCard({ theme, onThemeChange }: AppearanceCardProps) {
  return (
    <div className="rounded-2xl border border-border bg-card p-4">
      <div className="mb-4 flex items-center gap-2">
        <div className="rounded-md bg-sage-bg p-1.5">
          <Sun className="h-4 w-4 text-sage" />
        </div>
        <div>
          <h3 className="font-medium">Appearance</h3>
          <p className="text-xs text-muted-foreground">Light, dark, or follow your system</p>
        </div>
      </div>
      <FieldGroup className="gap-4">
        <Field orientation="responsive" className="items-center gap-4">
          <FieldContent>
            <FieldTitle>Theme</FieldTitle>
            <FieldDescription>Light, dark, or follow your system.</FieldDescription>
          </FieldContent>
          <Select
            items={[
              { value: "system", label: "System" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ]}
            value={normalizeTheme(theme)}
            onValueChange={(value) => onThemeChange(normalizeTheme(value))}
          >
            <SelectTrigger className="w-full md:w-[190px]" aria-label="Theme">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="system">System</SelectItem>
              <SelectItem value="light">Light</SelectItem>
              <SelectItem value="dark">Dark</SelectItem>
            </SelectContent>
          </Select>
        </Field>
      </FieldGroup>
    </div>
  );
}
