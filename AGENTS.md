# Project Guide for AI Agents

This project bootstraps the in-house Rust framework with common tooling and
patterns.

## Development

Run `./dev/test.sh` to render the template to `./target/demo_base` and create an
identical editable copy at `./target/demo_edit`. Make exploratory changes only
in `demo_edit`, then run `./dev/diff.sh`. The diff is printed and written to
`./target/demo.patch`; translate that patch back into the placeholder-aware
template sources. Re-render afterward and run formatting, Clippy, and tests in
the generated project.
