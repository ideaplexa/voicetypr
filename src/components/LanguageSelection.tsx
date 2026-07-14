"use client"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import type { SpeechModelEngine } from "@/types"
import { cn } from "@/lib/utils"
import { Check, ChevronsUpDown } from "lucide-react"
import * as React from "react"

// Languages sorted alphabetically by their display name for better UX
export const languages = [
  { value: "af", label: "Afrikaans" },
  { value: "sq", label: "Albanian" },
  { value: "am", label: "Amharic" },
  { value: "ar", label: "Arabic" },
  { value: "hy", label: "Armenian" },
  { value: "as", label: "Assamese" },
  { value: "az", label: "Azerbaijani" },
  { value: "ba", label: "Bashkir" },
  { value: "eu", label: "Basque" },
  { value: "be", label: "Belarusian" },
  { value: "bn", label: "Bengali" },
  { value: "bs", label: "Bosnian" },
  { value: "br", label: "Breton" },
  { value: "bg", label: "Bulgarian" },
  { value: "yue", label: "Cantonese" },
  { value: "ca", label: "Catalan" },
  { value: "zh", label: "Chinese" },
  { value: "hr", label: "Croatian" },
  { value: "cs", label: "Czech" },
  { value: "da", label: "Danish" },
  { value: "nl", label: "Dutch" },
  { value: "en", label: "English" },
  { value: "et", label: "Estonian" },
  { value: "fo", label: "Faroese" },
  { value: "fi", label: "Finnish" },
  { value: "fr", label: "French" },
  { value: "gl", label: "Galician" },
  { value: "ka", label: "Georgian" },
  { value: "de", label: "German" },
  { value: "el", label: "Greek" },
  { value: "gu", label: "Gujarati" },
  { value: "ht", label: "Haitian Creole" },
  { value: "ha", label: "Hausa" },
  { value: "haw", label: "Hawaiian" },
  { value: "he", label: "Hebrew" },
  { value: "hi", label: "Hindi" },
  { value: "hu", label: "Hungarian" },
  { value: "is", label: "Icelandic" },
  { value: "id", label: "Indonesian" },
  { value: "it", label: "Italian" },
  { value: "ja", label: "Japanese" },
  { value: "jw", label: "Javanese" },
  { value: "kn", label: "Kannada" },
  { value: "kk", label: "Kazakh" },
  { value: "km", label: "Khmer" },
  { value: "ko", label: "Korean" },
  { value: "lo", label: "Lao" },
  { value: "la", label: "Latin" },
  { value: "lv", label: "Latvian" },
  { value: "ln", label: "Lingala" },
  { value: "lt", label: "Lithuanian" },
  { value: "lb", label: "Luxembourgish" },
  { value: "mk", label: "Macedonian" },
  { value: "mg", label: "Malagasy" },
  { value: "ms", label: "Malay" },
  { value: "ml", label: "Malayalam" },
  { value: "mt", label: "Maltese" },
  { value: "mi", label: "Maori" },
  { value: "mr", label: "Marathi" },
  { value: "mn", label: "Mongolian" },
  { value: "my", label: "Myanmar" },
  { value: "ne", label: "Nepali" },
  { value: "no", label: "Norwegian" },
  { value: "nn", label: "Nynorsk" },
  { value: "oc", label: "Occitan" },
  { value: "ps", label: "Pashto" },
  { value: "fa", label: "Persian" },
  { value: "pl", label: "Polish" },
  { value: "pt", label: "Portuguese" },
  { value: "pa", label: "Punjabi" },
  { value: "ro", label: "Romanian" },
  { value: "ru", label: "Russian" },
  { value: "sa", label: "Sanskrit" },
  { value: "sr", label: "Serbian" },
  { value: "sn", label: "Shona" },
  { value: "sd", label: "Sindhi" },
  { value: "si", label: "Sinhala" },
  { value: "sk", label: "Slovak" },
  { value: "sl", label: "Slovenian" },
  { value: "so", label: "Somali" },
  { value: "es", label: "Spanish" },
  { value: "su", label: "Sundanese" },
  { value: "sw", label: "Swahili" },
  { value: "sv", label: "Swedish" },
  { value: "tl", label: "Tagalog" },
  { value: "tg", label: "Tajik" },
  { value: "ta", label: "Tamil" },
  { value: "tt", label: "Tatar" },
  { value: "te", label: "Telugu" },
  { value: "th", label: "Thai" },
  { value: "bo", label: "Tibetan" },
  { value: "tr", label: "Turkish" },
  { value: "tk", label: "Turkmen" },
  { value: "uk", label: "Ukrainian" },
  { value: "ur", label: "Urdu" },
  { value: "uz", label: "Uzbek" },
  { value: "vi", label: "Vietnamese" },
  { value: "cy", label: "Welsh" },
  { value: "yi", label: "Yiddish" },
  { value: "yo", label: "Yoruba" },
]

interface LanguageSelectionProps {
  value: string
  onValueChange: (value: string) => void
  className?: string
  engine?: SpeechModelEngine
  englishOnly?: boolean
  supportedLanguages?: readonly string[]
}

export function LanguageSelection({ value, onValueChange, className, engine = 'whisper', englishOnly = false, supportedLanguages }: LanguageSelectionProps) {
  const [open, setOpen] = React.useState(false)

  // Parakeet v3 supports 25 European languages
  const parakeetAllowed = React.useMemo(() => new Set([
    'bg','cs','da','de','el','en','es','et','fi','fr','hr','hu','it','lt','lv','mt','nl','pl','pt','ro','ru','sk','sl','sv','uk'
  ]), [])

  // Soniox supported languages (static list per docs). Keep in sync with codes in `languages` above.
  const sonioxAllowed = React.useMemo(() => new Set<string>([
    'en','es','fr','de','it','pt','nl','ru','zh','ja','ko','ar','hi','tr','pl','sv','no','da','fi','el','cs','ro','hu','sk','uk','he','id','vi','th','ms','tl','fa','ur','bn','ta','te','gu','pa','bg','hr','sr','sl','lv','lt','et','is','ca','gl'
  ]), [])

  // Cohere Transcribe supports 14 languages and does not auto-detect unsupported languages.
  const cohereAllowed = React.useMemo(() => new Set<string>([
    'en','de','fr','it','es','pt','el','nl','pl','vi','zh','ar','ja','ko'
  ]), [])

  const displayed = React.useMemo(() => {
    if (englishOnly) {
      return languages.filter(l => l.value === 'en')
    }
    if (supportedLanguages) {
      return languages.filter(l => supportedLanguages.includes(l.value))
    }
    if (engine === 'parakeet') {
      return languages.filter(l => parakeetAllowed.has(l.value))
    }
    if (engine === 'soniox') {
      return languages.filter(l => sonioxAllowed.has(l.value))
    }
    if (engine === 'cohere') {
      return languages.filter(l => cohereAllowed.has(l.value))
    }
    return languages
  }, [engine, parakeetAllowed, sonioxAllowed, cohereAllowed, englishOnly, supportedLanguages])
  
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          disabled={englishOnly}
          className={cn("w-48 justify-between", className)}
        >
          {englishOnly
            ? "English"
            : value
              ? languages.find((language) => language.value === value)?.label
              : "Select language"}
          <ChevronsUpDown className="opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[200px] p-0">
        <Command>
          <CommandInput placeholder="Search language..." className="h-9" />
          <CommandList>
            <CommandEmpty>No language found.</CommandEmpty>
            <CommandGroup>
              {displayed.map((language) => (
                <CommandItem
                  key={language.value}
                  // Use label for search instead of value so users can search by language name
                  value={language.label}
                  onSelect={() => {
                    // Pass the actual language code (value) when selected
                    onValueChange(language.value)
                    setOpen(false)
                  }}
                >
                  {language.label}
                  <Check
                    className={cn(
                      "ml-auto",
                      value === language.value ? "opacity-100" : "opacity-0"
                    )}
                  />
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}
