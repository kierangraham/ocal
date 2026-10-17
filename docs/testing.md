# Testing

Run every check from the repository root:

```sh
make lint    # rustfmt check, then clippy with warnings as errors
make check   # cargo check for all targets
make test    # all tests
```

GitHub Actions runs the same checks on every push and pull request. See `.github/workflows/ci.yml`.

## What the tests cover

| Crate | Tests | Covers |
| --- | --- | --- |
| `ocal-core` | 31 | Storage, recurrence, time zones, iCalendar, OAuth and Google Calendar requests |
| `ocal-app` | 50 | Keyboard and pointer interaction, views, the composer, details, settings and sign-in |

**`ocal-core` tests:**

- **Storage.** SQLite persistence. A failed import writes nothing.
- **Layout and selection.** Overlapping events and ranges that cross midnight or run backwards.
- **Recurrence and time zones.** Weekly rules across daylight-saving changes, and monthly rules that skip invalid dates.
- **iCalendar.** Line folding, all-day round trips, and errors for unsupported rules.
- **OAuth.** State and callback checks, PKCE, stale or expired attempts, and cancelling.
- **Google Calendar.** Request payloads, Meet requests, ETag conflicts and API errors. Each Google account keeps its own data.

**`ocal-app` tests** drive the real GPUI views without a window:

- **Creating events.** The time cursor, Shift ranges, Shift-drag and context menus.
- **Panels.** The composer and the details card.
- **Feedback.** Toasts, including Undo.
- **Small windows.** Layouts at 480 × 480 and 180% zoom.
- **Sign-in.** Copying the sign-in link, errors beside the button, and several accounts.

## What the tests don't cover

**No test talks to Google.** Google tests use local HTTP fixtures and never contact Google. Check these by hand with a real account after changing sync or sign-in code:

- signing in
- syncing more than one account
- creating, editing and deleting events
- invitations
- generating Meet links

**Visual checks are manual.** Run `make demo` and compare each view in light and dark themes.
