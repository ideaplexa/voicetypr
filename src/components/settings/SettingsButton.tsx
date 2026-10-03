import type { ComponentProps } from "react";
import { Button as PrimitiveButton } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function Button({ className, ...props }: ComponentProps<typeof PrimitiveButton>) {
  return <PrimitiveButton {...props} className={cn("rounded-[10px] focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2", className)} />;
}
