import type { ComponentProps } from "react";
import { Switch as PrimitiveSwitch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

export function Switch({ className, size = "default", ...props }: ComponentProps<typeof PrimitiveSwitch>) {
  return (
    <PrimitiveSwitch
      {...props}
      size={size}
      className={cn(
        "data-checked:bg-sage data-[size=default]:h-5 data-[size=default]:w-[34px] border-0 [&_[data-slot=switch-thumb]]:bg-white [&_[data-slot=switch-thumb]]:dark:bg-white",
        size === "sm"
          ? "p-px [&_[data-slot=switch-thumb][data-checked]]:translate-x-[10px]"
          : "p-0.5 [&_[data-slot=switch-thumb][data-checked]]:translate-x-[14px]",
        className,
      )}
    />
  );
}
