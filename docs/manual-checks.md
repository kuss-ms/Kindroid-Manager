# Manual end-to-end checklist

> **Note** The checklist previously lived in `AGENTS.md §Manual end-to-end checklist (matches the README)`. It moved here so `AGENTS.md` stays focused on toolchain, layout, conventions, and troubleshooting. The README used to mirror this content; it no longer does, and this file is now the single source of truth.
>
> **Note on numbering** Entries 31 and 32 each appear twice in the original text (a numbering bug from an old share-image/export addition). All four items are preserved verbatim below; the second occurrence keeps the original duplicate number for traceability.

These are integration / smoke checks that the automated Rust + Vitest suite cannot reach: flows against the live Kindroid API, WebView-only UI states, or destructive round-trips. Run them before releasing a build.

1. Launch with no token → first-run banner appears.
2. Settings → paste a bogus token → **Test** → "Invalid or missing API key". Replace with a real token → "OK". Clear token → banner reappears.
3. Create a Character "Test Bot" with a unique backstory.
4. Add a Target with a real `ai_id`.
5. Push (no chat-break) → verify in Kindroid.
6. Edit the backstory, push with chat-break + greeting → verify both.
7. Export a share image (with journal entries embedded), reset the app's data folder, drop the share image on the main window → character reappears with all fields, journal entries, and the cover image intact.
8. From History, click **Re-push** on the last entry → Push page pre-filled.
9. Disconnect the network, push → `(network)` error toast; log records the failure.
10. Push a character whose `user_name` differs from the AI's existing user → confirm the checklist warning is visible (still supported as a read-only field on existing data).
11. On the Push page, enable chat-break on a character that has a stored greeting → confirm the textarea is pre-filled and the Push button is enabled.
12. From the Characters overview, click **Share** on a character with a cover image → confirm the PNG is on the system clipboard.
13. Toggle "Reset Cascaded Memory" → confirm the warning callout appears explaining the data loss risk.
14. Click **Duplicate** on a character with a cover image → confirm the duplicate also shows the image in the edit screen.
15. Open **Chat History** with no targets → "Add a target on the Targets page…" empty state.
16. Add a target → return to Chat History, select it. Click **Sync** → status flips to "Syncing…", counter advances. Cancel mid-loop → state becomes `Cancelled`; clicking Sync resumes from the saved cursor.
17. Search for a word in the search bar → top hits render with a snippet; search "run" finds "running"/"runs" (Porter stem). Two words (`hello world`) require both terms; `"hello world"` (quoted) is an exact phrase match.
18. Disconnect the network, click Sync → state becomes `Error` with a message; click Sync again to retry.
19. With target A syncing, open Chat History for target B → action disabled, "Sync in progress for A". Cancel A to sync B.
20. Click the heart on a row → row updates instantly; the same message in the Kindroid web UI shows pinned.
21. With network disabled → click heart → row updates → ~1 s later reverts and error toast appears.
22. Toggle "Favourites only" → only pinned rows render in both browse and search modes.
23. Pin a message, run Sync → pin state survives.
24. Delete the target → pinned messages are gone (FK CASCADE applies).
25. Open a character with 3 journal entries → Push page lists them with checkboxes. Select 2, push → `/journal-create` is called twice in id order; both 200 → result shows two green `journal:` rows.
26. Re-push a log entry that originally pushed 2 journal entries → Push page pre-selects those 2 ids via the URL param.
27. Push a character with chat-break enabled and journal entries selected → order on the server is `update-info` → `journal-create ×N` → `chat-break`; all visible in the result block.
28. Push with `update-info` failing (network off) → no journal calls fire; only `update-info` is shown in the result, error toast appears.
29. Editor: try to save an entry with 9 keyphrases → error toast "at most 8 keyphrases"; counter shows `8/8` and the 9th is rejected client-side as well.
30. Editor: save an entry with 501 characters → error toast "entry must be 500 characters or fewer".
31. Editor: save an entry with a comma-separated keyphrase like `dragon wings, forked tongue` → error toast "keyphrase must not contain separators".
32. Editor: save an entry with a multi-word keyphrase like `purple skin` → accepted (1..3 words allowed); save with 4+ words like `one two three four` → error toast "keyphrase must be 3 words or fewer".
33. Export a character with 5 journal entries as a share image → reset app data → drop the image → character reappears with 5 journal entries (entry text + keyphrases preserved; ids and timestamps are new).
34. Delete a character with journal entries → entries are gone (FK CASCADE).
35. From Characters overview, click **Push as new Kin** on a character with `ai_name`, no journal entries → confirm; toast shows `New Kin created with ai_id …`; Push History detail lists `create-new-ai response` (status 200) and `update-info response` (status 200); Targets list now contains a row with the new ai_id and the AI name as label.
36. Sync a target with fewer than 10 messages → automation does not process them; add stable messages and confirm the newest 10 remain excluded.
37. Enable auto-journal after a completed sync → no historical backfill occurs; after the configured interval of stable messages, generated entries are sent to Kindroid.
38. Force a partial `journal-create` failure → successful entries remain sent, the failed entry is retried on the next completed sync, and no successful entry is regenerated.
39. Enable auto-summary with **Bootstrap from existing history** → the next completed sync summarizes all stable history; switch to **Incremental only** and confirm no initial AI call occurs.
40. Add enough new stable messages for an incremental summary → the selected Kindroid field is updated; switch backend with an over-limit summary and confirm the reformat path runs before the remote update.
41. Click **Reset summary** → local summary, candidate, and cursor clear while auto-journal settings and audit entries remain; **Run summary now** respects incremental-only no-op behavior.
42. Set global automation instructions, then set a target override → prompts use override first, global second, and hard-coded defaults when both are empty; restore and clear each override.
43. Configure an authless AI endpoint and an authenticated endpoint → automation sends an explicit empty AI bearer for the former and the stored bearer for the latter; no token appears in logs.
44. Delete a target with automation enabled → automation state, pending runs, and generated audit entries are removed by cascade.
45. Add a group target on the Targets page → row shows `Group chat` badge.
46. Edit the target → kind radio is disabled.
47. Push page → the group target does NOT appear in the dropdown.
48. Chat History → select the group target → Sync fetches; messages list populates.
49. Pin a message → row reflects favourite; Kindroid web UI shows the pin.
50. On a group target, the `Automation…` button is disabled.
51. Delete an AI target that has chat history → all rows cascade (existing behaviour).
52. Delete a group target with chat history → same cascade.
53. Add an AI and a Group with the same `id` string → both rows coexist; sync state and chat_messages are scoped to (id, kind).
54. Character with no default → editor shows "— none —".
55. Pick an AI target → save → reopen → still selected.
56. Push page entry (no URL `targetId`) → dropdown pre-selected with the character's default.
57. "Push as new Kin" on character with no default → reopen editor → default now points at the just-created target.
58. Repeat 56 on a character that ALREADY has a default → default unchanged.
59. Delete a referenced target → Targets row had "Default for N character(s)" caption before; affected character editor now shows "— none —".
60. Duplicate a character with a default → duplicate's editor shows the same default.
61. `/push?characterId=X` from character whose default is T → dropdown shows T. Same page with `?targetId=U` (Re-push link) → dropdown shows U.
62. Editor dropdown lists only AI targets; group targets absent.
63. On Push page, manually clear the dropdown → trigger any character refetch → dropdown stays cleared (does not snap back to the default).
64. Editor → character with `default_target_id` set + populated `ai_directive`. Click the per-field "Push" button next to "Response directive". Confirm Kindroid updates, success toast names the target, push-history gains an entry, and the default-target label is unchanged (since `ai_name` was not pushed).
65. Same editor, click the per-field "Push" button next to "Name" → confirm the target's local label on the Targets page syncs to the new `ai_name`.
66. Editor → character with no default target. Confirm all 8 per-field "Push" buttons are disabled with the "Set a default push target" tooltip. Pick a default → buttons enable.

## Global font-scale adjuster

1. Cleared localStorage: app loads at Medium. No font flash on first paint.
2. Settings → Appearance → click each preset (Small / Medium / Large / Extra Large). Text on every page (Characters, Editor, Targets, Push, History, Chat History, Settings, AutomationPanel) updates immediately. Header / buttons / modals don't clip.
3. Reload the app. Selected preset persists. No flash of default before saved scale.
4. Switch theme (light / dark / system). Font size is unaffected. Dark tokens are unaffected.
5. Mobile bottom-nav viewport (≤ 720 px): text scales inside nav links; nav links do not overflow at Extra Large (known tight at 1.25× — see "Risks" in `.kilo/plans/1786307570997-global-font-size-adjuster.md`).
6. Inline-styled paragraphs (Character Editor, Push, Settings cards) scale with the rest (they use `.text-xs` / `.text-sm` / `.text-md` utility classes, all rem-based).
7. Future PR adds a `font-size: NNpx` to `global.css` → REJECT. Every font-size in `global.css` MUST be in `rem` so it inherits the global scale. The comment above the `html { font-size: ... }` rule documents this.

## Chat mode (single-AI composer / rewind / suggest)

1. Open chat-view for an AI target. Send 3 messages in a row.
2. ✨ Suggest on an empty composer → textarea fills, text is selected. Type over a few chars, press Enter → sends.
3. ✨ Suggest repeatedly → button disables for ~1.5 s each call.
4. Switch to History. All 3 sent messages visible.
5. Click Sync. After the first tick, open History detail on one of the 3 messages → `kindroid_msg_id` is the real server id (no `local:` prefix); no duplicate rows.
6. Switch back to Chat. Rewind 2. Last two messages disappear locally; no orphaned synthetic twins.
7. Toggle themes (preset + custom). CSS custom properties on `.chat-view` update live.
8. Sync cancels on chat entry (when active) and resumes via Sync. Toast "Sync paused for chat." appears.
9. Send disabled when textarea is whitespace-only.
10. Group target → segmented control hidden; ChatView never renders.
