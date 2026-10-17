# Validation

Validated on 17 October 2026, after the redesign, on Arch Linux ARM / Omarchy, aarch64, Rust 1.97.0.

- `cargo fmt --all -- --check`
- `cargo test --locked --workspace`: **81 tests passed** (31 core, 50 native UI interactions).
- `cargo clippy --locked --workspace --all-targets -- -D warnings`
- Release build and desktop-entry validation.

The UI tests cover 15-minute keyboard selection, Shift-drag in empty calendar space, input isolation, disabling edits during save, retry after errors, returning focus after save, context-menu keyboard ownership and dismissal, right-click placement and time selection, full-width week/month layouts, explicit Meet generation with stable retry IDs, pasted-link focus, removing conferences, and 480×480 light-theme layouts at 180% zoom with focused fields revealed by scrolling. Sign-in tests cover the hidden custom-client option, disabled demo sign-in, progress staying busy, retry after cancellation, the ordinary Google button dispatching without a file picker, sign-in errors beside the button, successful connection returning to the calendar, grouped Google accounts, per-account calendar visibility, adding another account, the event editor saving to the selected account, account cards fitting 480×480 windows, the original authorization link reaching the clipboard, copying again retaining the same attempt, cancellation/retry, and link controls being removed after callback or expiry. The ordinary-button test was also run in a separate build with a synthetic publisher client ID; it uses an isolated command channel and does not open a browser or contact Google.

Redesign tests cover the sheet and toolbar layout, tooltips with lower-case keys, toasts (Undo runs once, info fades, errors stay), the sync footer, the light/dark preference, the shortcuts card, the details card sitting beside its event and flipping near the right edge (a full-width panel in compact windows), closing it with a second click or an outside click, Enter editing the open event, deleting with Undo and sending the delete only once the message goes or the app quits, the composer centred over the sheet with calendar tabs and More details, tasks with checkboxes in the month, agenda and all-day row, month dates opening the day, sticky agenda day headings and the overlap cascade.

Core tests cover SQLite persistence and atomic failed imports, overlap lanes, DST-preserving weekly recurrence, skipping invalid monthly dates while respecting occurrence counts, ambiguous DST rejection, selection across midnight/in reverse, iCalendar folding/date-only round trips/unsupported recurrence errors, OAuth state and callback validation, missing/invalid publisher-client rejection and optional native-client secrets in token requests, Google all-day ranges, recurring-instance patches and Meet request payloads. Local HTTP fixtures verify Calendar API error decoding for disabled API, missing permissions, rate limits, malformed error responses and ETag conflicts without exposing arbitrary provider data. Multi-account checks cover the account chooser retaining PKCE/offline access, shared calendar/event IDs remaining distinct per account, reconnecting/syncing one account preserving other accounts and local data, and an expired account not stopping subsequent accounts from syncing. Local loopback HTTP tests verify that a stale callback gets rejected while the current attempt remains usable, matched Google denials cancel cleanly, expired attempts require a fresh link, and preparing a copied link does not launch a browser or expose the client secret/PKCE verifier.

Native visual review ran the `--demo` build as floating 1440×900 Wayland windows on the desktop, with the real GPUI renderer, native text and the current Omarchy theme; the captures are not HTML mockups. Each view was compared side by side with the build before the redesign and with the [prototype](prototype/redesign.html). That review found and fixed month dates stretching across their cell and the date picker running over the composer's start time. Context menus, the meeting choices and compact windows were not recaptured; they are covered by UI tests.

Keyboard regressions additionally check the two-pixel time cursor, highlighting and opening existing events, exclusive event end times, overlap targeting, calendar visibility, event context actions, Shift selection through occupied time, reverse selection and collapsing back to the anchor. Rendered event bounds are checked against their actual duration for 5-, 15-, 30-, 60- and 90-minute events, including overnight clipping, at 70%, 100% and 180% zoom. Keyboard selection uses time boundaries; pointer drags retain inclusive 15-minute slots.

## Captures

- [Week](week.png), with the Now card and today's running event
- [Day](day.png)
- [Event details](event-details.png)
- [Event editor](event-editor.png)
- [Month](month.png)
- [Agenda](agenda.png)
- [Tasks](tasks.png)
- [Light mode](light.png), with an event's details open

## Live Google checks remaining

A Desktop OAuth client from Downloads is configured locally for builds; its metadata is in a Git-ignored Cargo config and is not recorded here. Live browser authorization completed, but the first Calendar API request returned HTTP 403 and no Google account was saved in the database during inspection. The previous UI hid this error in a small footer and reported local-only refreshes as “Calendars synced”; both behaviors are corrected. The actual 403 reason was discarded by the previous build, so a disabled API remains a hypothesis until confirmed by a retry. No real Google event, invitation or conference was created during validation. API payload and UI tests do not substitute for live verification. Before treating this build as a daily Google client, verify copied-link browser OAuth in the desired browser profile with the configured Desktop app client, connect at least two real Google accounts, reconnect either without duplicates, verify shared-calendar destinations and continued sync after one account fails, multiple calendars/account policy, paginated sync, read-only access, creation/edit/deletion, ETag conflicts, guest invitations and RSVP, pending/success/failure Meet generation, removing conferences and recovering after a network interruption.

The cache is a rolling window; sync tokens, watch notifications, offline writes, remote calendar removal and account-disconnect UI are unfinished. Reminder delivery is also unfinished. See [the feature matrix](features.md) for the remaining Fantastical parity work.
