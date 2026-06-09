import { describe, expect, it } from "vitest";
import {
  derivePostprocLanguageFromAsr,
  resolveEffectiveAsrLanguageHint,
} from "../language-utils";

describe("resolveEffectiveAsrLanguageHint", () => {
  it("returns pinned language when pinned", () => {
    expect(resolveEffectiveAsrLanguageHint("en", true)).toBe("en");
  });

  it("returns auto when unpinned", () => {
    expect(resolveEffectiveAsrLanguageHint("de", false)).toBe("auto");
  });

  it("normalizes uppercase language code", () => {
    expect(resolveEffectiveAsrLanguageHint("EN", true)).toBe("en");
  });

  it("defaults empty language to auto", () => {
    expect(resolveEffectiveAsrLanguageHint("", true)).toBe("auto");
  });
});

describe("derivePostprocLanguageFromAsr", () => {
  it("returns multi when language is not pinned", () => {
    expect(derivePostprocLanguageFromAsr("en", false)).toBe("multi");
  });

  it("returns en when pinned to English", () => {
    expect(derivePostprocLanguageFromAsr("en", true)).toBe("en");
  });

  it("returns de when pinned to German", () => {
    expect(derivePostprocLanguageFromAsr("de", true)).toBe("de");
  });

  it("returns multi for non en/de value", () => {
    expect(derivePostprocLanguageFromAsr("fr", true)).toBe("multi");
  });

  it("normalizes uppercase language code", () => {
    expect(derivePostprocLanguageFromAsr("DE", true)).toBe("de");
  });
});