# Lightweight Lapce Goal

## Objective

Build a lightweight Lapce-based code viewer and basic editor for local
repositories. It should provide file-tree browsing, basic syntax highlighting,
full Git workflows, a terminal, and a maintainable internationalized UI.

## In scope

- File-tree browsing and opening files.
- Basic editing and syntax highlighting supplied by the existing editor core.
- The existing Git integration, including viewing and normal workflows.
- The existing terminal integration.
- UI internationalization, starting with English and Simplified Chinese.
- A responsive Floem UI: user-visible translated text must be derived inside
  reactive view closures so a language change updates the view.

## Out of scope

- Plugin installation or execution.
- LSP, code intelligence, and language-server management.
- Debugging and DAP workflows.
- Remote development or remote workspace management.
- Replacing the editor core or Git implementation without a concrete need.

## Milestones

- [ ] Establish the runnable MVP shell and retain the existing editor core.
- [ ] Complete the i18n foundation and migrate all user-visible UI text.
- [ ] Verify file-tree browsing, opening, basic editing, and syntax highlighting.
- [ ] Verify the existing Git views and workflows without feature reduction.
- [ ] Verify terminal creation, switching, and interaction.
- [ ] Disable or remove out-of-scope plugin, LSP, debug, and remote entry points.
- [ ] Run formatting, focused tests, and a full build with the pinned toolchain.

## Working rules for LLM follow-up

1. Read `AGENTS.md`, this file, and `git status` before editing.
2. Make the smallest change that advances the current unchecked milestone.
3. Preserve user changes and avoid unrelated dependency or architecture changes.
4. Keep persisted identifiers, command IDs, file names, Git refs, and source
   content language-neutral; translate only user-facing UI text.
5. Prefer existing Lapce/Floem components and reactive state over parallel
   abstractions.
6. After editing, run the narrowest useful checks and record any unavailable
   verification in the handoff.
7. Update this checklist only when there is concrete evidence that a milestone
   is complete; leave the next actionable item clear.

## Definition of done

The tool opens a local repository, lets a user browse and edit files with basic
syntax highlighting, inspect and use Git, open a terminal, and switch supported
UI languages. Out-of-scope development features are unavailable from the
normal UI, and the pinned Rust toolchain can build and test the workspace.
