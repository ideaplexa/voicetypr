import { SettingsPage, PageHeader } from "@/components/settings/settings-ui";

export function InsightsTab() {
  return (
    <SettingsPage>
      <PageHeader title="Insights" />
      <p className="text-[13px] text-muted-foreground">Coming in this build</p>
    </SettingsPage>
  );
}
