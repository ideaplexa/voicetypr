import { AccountSection } from "@/components/sections/AccountSection";

export function AccountTab({ embedded = false }: { embedded?: boolean } = {}) {
  return <AccountSection embedded={embedded} />;
}
