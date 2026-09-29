import { Toaster } from "@/components/ui/sonner";
import { useTheme } from "@/hooks/useTheme";
import type { ComponentProps } from "react";

export function AppToaster(props: ComponentProps<typeof Toaster>) {
  const theme = useTheme();
  return <Toaster {...props} theme={theme} />;
}
