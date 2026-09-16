#!/usr/bin/env python3
"""Extract and audit Lapce's translation catalog.

The application uses TOML locale files and a small Rust/Floem wrapper rather
than a framework-specific extraction format. This script uses source-shape
regular expressions to collect stable translation references from UI calls,
settings metadata, and Lapce/Floem command annotations. It intentionally does
not treat user content, paths, diagnostics, plugin metadata, command IDs, or
configuration values as translatable text.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python 3.10 fallback
    print("i18n_audit.py requires Python 3.11+ (tomllib)", file=sys.stderr)
    raise


ROOT = Path(__file__).resolve().parents[1]
LOCALE_DIR = ROOT / "lapce-app" / "assets" / "locales"
EN_PATH = LOCALE_DIR / "en.toml"
ZH_PATH = LOCALE_DIR / "zh-CN.toml"

CONFIG_KINDS = ("core", "editor", "ui", "terminal")
CONFIG_FIELDS = re.compile(
    r"#\[field_names\((?P<attrs>.*?)\)\]\s*"
    r"(?P<visibility>pub\s+)?(?P<field>[A-Za-z_][A-Za-z0-9_]*)\s*:",
    re.DOTALL,
)
CONFIG_STRUCT = re.compile(
    r"pub\s+struct\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*\{(?P<body>.*?)\n\}",
    re.DOTALL,
)
I18N_KEYS = re.compile(
    r"\b(?:text|text_signal)\(\s*\"(?P<key>[A-Za-z0-9_.-]+)\""
)
RAW_UI_LITERALS = re.compile(
    r"(?P<call>(?<![\w.])(?:text|label|Menu::new|MenuItem::new)\s*\(\s*"
    r"|\.title\s*\(\s*|\.placeholder\s*\(\s*)"
    r"\"(?P<text>(?:\\.|[^\"\\])+)\""
)
STRUM_COMMANDS = re.compile(
    r"(?P<attrs>(?:\s*#\[strum\([^\]]*\)\]\s*)+)"
    r"(?P<variant>[A-Za-z_][A-Za-z0-9_]*)\b",
    re.MULTILINE,
)
STRUM_ATTRIBUTES = re.compile(
    r"\b(?P<name>serialize|message)\s*=\s*\"(?P<value>(?:\\.|[^\"\\])*)\""
)


@dataclass(frozen=True)
class SourceEntry:
    key: str
    source_kind: str
    source_file: str
    source_line: int
    source_symbol: str
    source_english: str = ""


@dataclass(frozen=True)
class LiteralCandidate:
    text: str
    source_file: str
    source_line: int
    context: str


def relative_path(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        if path.name == "command.rs" and path.parent.name == "src":
            if path.parent.parent.name == "editor-core":
                return "floem-editor-core/src/command.rs"
        return path.resolve().as_posix()


def source_line(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def decode_string(value: str) -> str:
    return bytes(value, "utf-8").decode("unicode_escape")


def untranslated_candidates(en: dict[str, str]) -> list[LiteralCandidate]:
    """Find literal UI strings that bypass the i18n wrapper.

    This is intentionally a candidate report: application names and a few
    platform/API values can look like UI text but should not be translated.
    Only obvious UI constructors are scanned, while debug names, logs, tests,
    and arbitrary string constants are left out.
    """
    candidates: list[LiteralCandidate] = []
    ignored = {"Lapce"}
    for source in (ROOT / "lapce-app" / "src").rglob("*.rs"):
        text = source.read_text(encoding="utf-8")
        for match in RAW_UI_LITERALS.finditer(text):
            value = decode_string(match.group("text"))
            if value in ignored or not re.search(r"[A-Za-z\u3400-\u9fff]", value):
                continue
            candidates.append(
                LiteralCandidate(
                    value,
                    relative_path(source),
                    source_line(text, match.start()),
                    match.group("call").strip(),
                )
            )
    return candidates


def parse_locale(path: Path) -> dict[str, str]:
    with path.open("rb") as source:
        data = tomllib.load(source)
    return {str(key): str(value) for key, value in data.items()}


def kebab(field: str) -> str:
    return field.replace("_", "-")


def settings_entries() -> Iterable[SourceEntry]:
    for kind in CONFIG_KINDS:
        source = ROOT / "lapce-app" / "src" / "config" / f"{kind}.rs"
        text = source.read_text(encoding="utf-8")
        struct_name = f"{kind.title()}Config"
        struct = next(
            (match for match in CONFIG_STRUCT.finditer(text) if match.group("name") == struct_name),
            None,
        )
        if struct is None:
            continue
        for match in CONFIG_FIELDS.finditer(struct.group("body")):
            attrs = match.group("attrs")
            if "skip" in attrs:
                continue
            field = kebab(match.group("field"))
            desc_match = re.search(r'desc\s*=\s*"((?:\\.|[^"\\])*)"', attrs)
            if not desc_match:
                continue
            base = f"settings.item.{kind}.{field}"
            line = source_line(text, struct.start() + match.start())
            description = decode_string(desc_match.group(1))
            yield SourceEntry(
                f"{base}.name",
                "settings-metadata",
                relative_path(source),
                line,
                field,
            )
            yield SourceEntry(
                f"{base}.description",
                "settings-metadata",
                relative_path(source),
                line,
                field,
                description,
            )


def literal_entries() -> Iterable[SourceEntry]:
    for source in (ROOT / "lapce-app" / "src").rglob("*.rs"):
        text = source.read_text(encoding="utf-8")
        for match in I18N_KEYS.finditer(text):
            yield SourceEntry(
                match.group("key"),
                "ui-i18n-call",
                relative_path(source),
                source_line(text, match.start()),
                "text/text_signal",
            )


def floem_command_source() -> Path | None:
    candidates = [
        ROOT.parent / "floem" / "editor-core" / "src" / "command.rs",
    ]
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    candidates.extend(
        cargo_home.glob("git/checkouts/floem-*/*/editor-core/src/command.rs")
    )
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    return None


def command_entries(source: Path, source_kind: str) -> Iterable[SourceEntry]:
    text = source.read_text(encoding="utf-8")
    for match in STRUM_COMMANDS.finditer(text):
        attributes = {
            attribute.group("name"): decode_string(attribute.group("value"))
            for attribute in STRUM_ATTRIBUTES.finditer(match.group("attrs"))
        }
        command_id = attributes.get("serialize")
        message = attributes.get("message")
        if not command_id or not message:
            continue
        yield SourceEntry(
            f"command.{command_id}",
            source_kind,
            relative_path(source),
            source_line(text, match.start()),
            match.group("variant"),
            message,
        )


def source_entries() -> list[SourceEntry]:
    entries = list(literal_entries())
    entries.extend(settings_entries())
    entries.extend(
        command_entries(
            ROOT / "lapce-app" / "src" / "command.rs",
            "lapce-command",
        )
    )
    floem = floem_command_source()
    if floem is not None:
        entries.extend(command_entries(floem, "floem-command"))
    return entries


def source_index(entries: Iterable[SourceEntry]) -> dict[str, SourceEntry]:
    index: dict[str, SourceEntry] = {}
    for entry in entries:
        index.setdefault(entry.key, entry)
    return index


def toml_string(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'


def generated_english(entry: SourceEntry) -> str:
    if entry.source_english:
        return entry.source_english
    if entry.key.startswith("settings.item.") and entry.key.endswith(".name"):
        field = entry.source_symbol.replace("-", " ")
        return field.replace("_", " ").title()
    return entry.key


def write_locale(path: Path, values: dict[str, str]) -> None:
    lines = [f"{toml_string(key)} = {toml_string(values[key])}" for key in sorted(values)]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def sync_locales(en: dict[str, str], zh: dict[str, str], entries: list[SourceEntry]) -> None:
    """Synchronize the two runtime locale tables without translating Chinese."""
    source_map = source_index(entries)
    keys = set(en) | set(zh) | set(source_map)
    generated_en = {
        key: en[key]
        if key in en
        else generated_english(source_map[key])
        for key in keys
    }
    generated_zh = {key: zh.get(key, "") for key in keys}
    write_locale(EN_PATH, generated_en)
    write_locale(ZH_PATH, generated_zh)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--sync-locales",
        action="store_true",
        help="update en.toml and zh-CN.toml from extracted source keys",
    )
    args = parser.parse_args()

    en = parse_locale(EN_PATH)
    zh = parse_locale(ZH_PATH)
    errors: list[str] = []

    if set(en) != set(zh):
        errors.extend(f"locale key mismatch: {key}" for key in sorted(set(en) ^ set(zh)))

    entries = source_entries()
    if args.sync_locales:
        sync_locales(en, zh, entries)
        en = parse_locale(EN_PATH)
        zh = parse_locale(ZH_PATH)
    expected_settings = {
        entry.key for entry in entries if entry.source_kind == "settings-metadata"
    }
    missing_settings = sorted(expected_settings - set(zh))
    if missing_settings:
        errors.extend(f"missing settings translation: {key}" for key in missing_settings)

    for key in sorted(expected_settings & set(zh)):
        value = zh[key]
        if not re.search(r"[\u3400-\u9fff]", value):
            errors.append(f"settings translation contains no Chinese text: {key}")

    expected_commands = {
        entry.key
        for entry in entries
        if entry.source_kind in {"lapce-command", "floem-command"}
    }
    for key in sorted(expected_commands):
        if key not in en:
            errors.append(f"missing English command translation: {key}")
        if key not in zh:
            errors.append(f"missing Simplified Chinese command translation: {key}")

    referenced = {
        entry.key for entry in entries if entry.source_kind == "ui-i18n-call"
    }
    missing_references = sorted(referenced - set(en))
    if missing_references:
        errors.extend(f"missing referenced translation key: {key}" for key in missing_references)

    candidates = untranslated_candidates(en)

    print(f"locale keys: en={len(en)} zh-CN={len(zh)}")
    print(f"settings metadata keys: {len(expected_settings)}")
    print(f"literal i18n keys found in Rust: {len(referenced)}")
    print(
        "command source entries: "
        f"lapce={sum(entry.source_kind == 'lapce-command' for entry in entries)} "
        f"floem={sum(entry.source_kind == 'floem-command' for entry in entries)}"
    )
    print(f"untranslated UI literal candidates: {len(candidates)}")
    for candidate in candidates:
        print(
            f"CANDIDATE: {candidate.source_file}:{candidate.source_line}: "
            f"{candidate.text!r} ({candidate.context})"
        )
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print("i18n audit: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
