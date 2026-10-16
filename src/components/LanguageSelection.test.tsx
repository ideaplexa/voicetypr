import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { LanguageSelection } from "@/components/LanguageSelection";

describe("automatic spoken language", () => {
  it.each(["whisper", "parakeet", "crispasr"] as const)(
    "offers Auto for multilingual %s even with a language filter",
    async (engine) => {
      const onValueChange = vi.fn();
      render(
        <LanguageSelection
          value="fr"
          engine={engine}
          supportedLanguages={["en", "fr"]}
          onValueChange={onValueChange}
        />,
      );
      await userEvent.click(screen.getByRole("combobox", { name: "Spoken language" }));
      await userEvent.click(await screen.findByRole("option", { name: "Auto" }));
      expect(onValueChange).toHaveBeenCalledWith("auto");
    },
  );

  it("labels a saved Auto choice", () => {
    render(<LanguageSelection value="auto" onValueChange={vi.fn()} />);
    expect(screen.getByRole("combobox", { name: "Spoken language" })).toHaveTextContent("Auto");
  });

  it("keeps English-only models explicit", () => {
    render(<LanguageSelection value="en" englishOnly onValueChange={vi.fn()} />);
    expect(screen.getByRole("combobox", { name: "Spoken language" })).toBeDisabled();
    expect(screen.getByRole("combobox", { name: "Spoken language" })).toHaveTextContent("English");
  });

  it("does not offer Auto for Cohere's explicit-language API", async () => {
    render(<LanguageSelection value="en" engine="cohere" onValueChange={vi.fn()} />);
    await userEvent.click(screen.getByRole("combobox", { name: "Spoken language" }));
    expect(await screen.findByRole("option", { name: "English" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Auto" })).not.toBeInTheDocument();
  });
});
