# Internationalization

The application keeps user interface translations in
`lapce-app/assets/locales`. Locale files use stable, dotted keys and are
embedded into the binary at compile time.

The `ui.language` setting accepts:

- `auto`, which checks the process locale environment and falls back to English;
- `en` for English;
- `zh-CN` for Simplified Chinese.

When a translation is missing, the lookup falls back to English and then to
the key itself. New keys must be added to every locale file. The
`locale_files_have_matching_keys` test checks this requirement.

Use the shared `I18n` instance from `CommonData` in views. Do not place
translated text in configuration, persisted workspace state, file names, or
Git references. User-provided content and source code remain unchanged.

For Floem views, prefer `label(i18n.text_signal("some.key"))` (or pass the
same producer to another text-bearing view) so the locale signal is tracked
and the text updates when the language changes. Calling `i18n.text(...)` while
constructing a view snapshots the current translation and should only be used
for non-reactive values. `I18n` is kept in `CommonData` rather than Floem's
context because the pinned Floem context store is runtime-global, while
Lapce allows window tabs to have independent workspace settings.
