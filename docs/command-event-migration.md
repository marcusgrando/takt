# Calendar values in commands

Calendar text must be passed as data because invited event titles and notes can contain executable syntax.

Command scripts and raw sh, bash, and zsh arguments reject `{{event.*}}` templates. Replace `echo "{{event.title}}"` with `printf '%s\n' "$TAKT_EVENT_TITLE"`.

Available environment variables are `TAKT_EVENT_TITLE`, `TAKT_EVENT_START`, `TAKT_EVENT_END`, `TAKT_EVENT_NOTES`, `TAKT_EVENT_LOCATION`, `TAKT_EVENT_URL`, `TAKT_EVENT_CONFERENCE_URL`, and `TAKT_EVENT_CALENDAR_ID`. Missing values and runs without an event receive empty strings. Quote shell expansions and never pass event values to `eval` or another interpreter as source code.

Python and AppleScript argument fields still support event templates as separate arguments. Their script fields must use environment access or read those arguments. User-authored shell syntax and raw shell arguments otherwise keep their existing behavior.

Commands have a five minute deadline and a 1 MiB limit on each output stream. The process group is terminated on completion, failure, timeout, or cancellation, including ordinary background children. Deliberately detached processes can escape a process group, so scripts must not daemonize work they expect Takt to manage.

Webhooks have a 30 second total deadline, a 10 second connection deadline, and a 1 MiB response limit. Exceeding a limit fails the execution with an explicit error.
