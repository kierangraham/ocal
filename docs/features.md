# Fantastical reference and implementation status

Reference: [Fantastical](https://flexibits.com/fantastical), [calendar views](https://flexibits.com/fantastical/help/calendar-views), [calendar sets](https://flexibits.com/fantastical/help/calendar-sets). The visual study included the full-calendar and event-entry screenshots. Ocal uses its own GPUI UI and Lucide icons.

| Area | Ocal build |
| --- | --- |
| Day, week, month, agenda | Implemented; narrow week layout becomes a single day |
| Mini calendar and upcoming sidebar | Implemented |
| Calendar colors, visibility and sets | Display, checkboxes and saved sets implemented; color/name editing and set deletion pending |
| Fast creation | Time cursor with event highlighting/Enter to open, Shift selection through events, Shift-drag, double-click, Add and context menus; accurate duration heights with compact short-event titles |
| Event editor | Explicit title, dates, times, calendar, all-day, location, notes, guests, recurrence and IANA zone |
| Meeting links | Paste any HTTP(S) link; request Google Meet on capable calendars; pending/error states and Join action |
| Google accounts | Multiple accounts, copied sign-in links for your preferred browser profile, account chooser, grouped sidebar and account-labelled event destinations; separate credentials/cache and independent account sync. A Desktop client is configured locally; live multi-account and public distribution verification still needed |
| Event operations | Create, edit, duplicate and delete with Undo (sent once the Undo message goes); remote ETag conflict checks |
| Invitations | Guest email entry and explicit send-updates choice; Yes/Maybe/No response |
| Recurrence | Local simple daily/weekly/monthly/yearly rules; Google expands instances; local edits affect the series, Google edits affect the selected instance |
| Advanced recurrence | Rule UI for specific weekdays, exceptions and this-and-following changes pending |
| Search | Cached title, location and notes search; no provider-wide history search |
| Tasks | Local dated tasks and completion; Google Tasks integration pending |
| Offline | Local calendars and cached reading; durable remote write queue pending |
| Time zones | IANA event zones, DST validation and display-zone presets; full world-clock UI pending |
| iCalendar | Basic import/export; advanced recurrence, attendees and alarm round-trip pending |
| Themes and keyboard | Native Omarchy theme integration, context menus, focus isolation and zoom |
| Moving/resizing existing events | Pending; Shift-drag currently creates events |
| Quarter/year views | Pending |
| Reminders and desktop notifications | Reminder metadata retained; notification delivery pending |
| Templates, attachments, travel time, weather | Pending |
| Scheduling proposals, openings, availability | Pending |
| CalDAV, iCloud, Exchange, Microsoft | Pending; Google first |
| Account removal/public OAuth client | Pending |
| Natural-language entry | Deliberately omitted by request |

This is not full Fantastical parity. The immediate bar is clear navigation, a calm adaptable layout and direct event entry, with working local storage and a reviewable Google implementation.
