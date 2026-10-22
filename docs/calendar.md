# Calendar URLs and Telegram

The sidebar order is **Tasks → Calendar → Finance**. Personal events and tasks work offline.
Calendar sources use iCalendar subscription URLs, with no provider login or desktop client.

## Subscribe to a calendar

1. Open **Calendar → Settings → Calendars**.
2. Press **+ Add Calendar URL**, enter a name and an iCalendar subscription link.
3. Set **Refresh Every** (default 60 seconds), then press **Save Settings** or Enter.

HTTPS, HTTP, and webcal links are supported. Webcal links use HTTPS. URLs are stored
in encrypted credentials, since private subscription URLs can contain access tokens.
Use Google Calendar's iCal link or Outlook's published ICS link, according to your
provider's sharing settings. Settings includes links to the providers' instructions.
Subscribed events are read only. Personal events remain editable. Reminder choices
survive refresh; failed downloads retain the previous cache. Changing or removing a
subscription discards its old cache, and late responses cannot restore it.
Esc or Cancel discards settings edits. Refresh Now refreshes saved subscriptions.

## Scheduling and scrolling

Calendar opens in Day view. The selector is ordered Day, Week, Month, Agenda. The previous/next buttons navigate dates; Today
returns to the current date. Create opens a personal event/task draft. Personal
items can repeat daily, weekly, or monthly, with an optional end date. Kanban task due dates are managed in Tasks.

Use the mouse wheel or a touchpad gesture over calendar content to scroll. The
toolbar stays visible, and scrollbars are hidden. Month cells maintain a readable
minimum height and scroll in smaller windows. Week/day views scroll through all
24 hours; overflowing all-day items have their own scroll area above the timeline.
Agenda lists the next 31 days.

## Telegram reminders

Open **Calendar → Settings → Notifications**, enable notifications, and enter your
Telegram bot token and chat ID. Each event/task can specify a reminder offset in
minutes, hours, or days. Amigo sends reminders while running. After restart it
sends a short summary for missed items. Successful deliveries are recorded to
avoid duplicates; unsuccessful attempts can retry. Repeating events have separate
receipts for each occurrence.

## Storage and compatibility

`~/.amigo/organizer.json` stores personal events, subscription metadata, cached
occurrences, refresh timing, and reminder receipts. `credentials.enc` stores URLs
and Telegram settings using AES-256-GCM; its separate owner-only key is
`credentials.key`. Confirmed changes autosave. Periodic directory polling is disabled.

Account management, OAuth connections, and Email integration have been removed.
Existing account metadata, drafts, and encrypted credentials are preserved as
inactive legacy data on saves. Former account calendars are hidden and do not
refresh or send reminders. Personal events and configured URL subscriptions
continue working.

## Preview

Run with isolated data:

```sh
cargo build --locked --example organizer_preview
AMIGO_DATA_FILE=/tmp/amigo-calendar-review/workspace.json \
  target/debug/examples/organizer_preview /tmp/amigo-calendar-review/screens --dark --compact
```

The fixture uses synthetic events and never downloads calendars or sends messages.
