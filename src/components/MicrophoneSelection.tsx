"use client";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { Check, ChevronsUpDown, Mic } from "lucide-react";
import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";

const log = createLogger("microphone");

interface MicrophoneSelectionProps {
  value?: string;
  onValueChange: (value: string | undefined) => void;
  className?: string;
}

export function MicrophoneSelection({ value, onValueChange, className }: MicrophoneSelectionProps) {
  const [open, setOpen] = React.useState(false);
  const [devices, setDevices] = React.useState<string[]>([]);
  const [loading, setLoading] = React.useState(false);

  // Always-fresh refs so the device-list sources below can validate the
  // selection without a parent-sync effect.
  const valueRef = React.useRef(value);
  const onValueChangeRef = React.useRef(onValueChange);
  React.useEffect(() => {
    valueRef.current = value;
    onValueChangeRef.current = onValueChange;
  });

  const applyDeviceList = (audioDevices: string[]) => {
    setDevices(audioDevices);
    const selected = valueRef.current;
    if (selected && audioDevices.length > 0 && !audioDevices.includes(selected)) {
      log.debug(`Selected device "${selected}" is no longer available, resetting to default`);
      toast.info(`${selected} is no longer available, switching to default microphone`);
      onValueChangeRef.current(undefined); // Reset to default
    }
  };

  // Fetch audio devices on mount and validate stored selection
  React.useEffect(() => {
    const initializeDevices = async () => {
      try {
        setLoading(true);

        // First, validate that any stored microphone still exists
        // This cleans up stale selections from previously connected devices
        const wasReset = await invoke<boolean>("validate_microphone_selection");
        if (wasReset) {
          log.debug("Stale microphone selection was reset to default");
          toast.info("Previously selected microphone is no longer available, using default");
        }

        // Then fetch current devices
        const audioDevices = await invoke<string[]>("get_audio_devices");
        log.debug("Fetched audio devices:", audioDevices);
        applyDeviceList(audioDevices);
      } catch (error) {
        log.error("Failed to initialize audio devices:", error);
        toast.error("Failed to load audio devices");
      } finally {
        setLoading(false);
      }
    };

    initializeDevices();

    const listenerPromise = listen<string[]>("audio-devices-updated", ({ payload }) => {
      log.debug("Audio devices updated:", payload);
      applyDeviceList(Array.isArray(payload) ? payload : []);
    }).catch((error) => {
      log.warn("Failed to listen for audio device updates:", error);
      return () => {};
    });

    return () => {
      listenerPromise
        ?.then((dispose) => {
          dispose();
        })
        .catch((error) => {
          log.warn("Failed to unsubscribe from audio device updates:", error);
        });
    };
  }, []);

  const handleDeviceSelect = async (deviceName: string | undefined) => {
    log.debug(`Selecting microphone: ${deviceName || "Default"}`);
    onValueChange(deviceName);
    setOpen(false);
  };

  // Show device name, or indicate if it's unavailable
  const displayValue = React.useMemo(() => {
    if (!value) return "Default";
    if (devices.includes(value)) return value;
    return `${value} (Not Available)`;
  }, [value, devices]);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        render={
          <Button
            variant="outline"
            role="combobox"
            aria-expanded={open}
            className={cn("w-64 justify-between", className)}
            disabled={loading}
          />
        }
      >
        <div className="flex items-center gap-2">
          <Mic className="h-4 w-4" />
          <span className="truncate">{displayValue}</span>
        </div>
        <ChevronsUpDown className="opacity-50" />
      </PopoverTrigger>
      <PopoverContent className="w-64 p-0">
        <Command>
          <CommandInput placeholder="Search microphone..." className="h-9" />
          <CommandList>
            <CommandEmpty>No microphone found.</CommandEmpty>
            <CommandGroup>
              {/* Default option */}
              <CommandItem
                key="default"
                value="default"
                onSelect={() => handleDeviceSelect(undefined)}
              >
                <Mic className="mr-2 h-4 w-4" />
                Default
                <Check className={cn("ml-auto", !value ? "opacity-100" : "opacity-0")} />
              </CommandItem>
              {/* Available devices */}
              {devices.map((device) => (
                <CommandItem
                  key={device}
                  value={device}
                  onSelect={() => handleDeviceSelect(device)}
                >
                  <Mic className="mr-2 h-4 w-4" />
                  <span className="truncate">{device}</span>
                  <Check
                    className={cn("ml-auto", value === device ? "opacity-100" : "opacity-0")}
                  />
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
