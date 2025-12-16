# Project Guide for AI Agents

This AGENTS.md file provides comprehensive guidance for AI agents working with
rust rust-template.  This project is used to bootstrap our in house rust framework 
with common tooling and patterns.  The development cycle is 

## Development

This template has scripts `./dev/test.sh` which tries to render the template to the `./target/demo_base` path.  It also makes a `./target/demo_edit/` path.  The `./dev/diff.sh` script is run when `./target/demo_edit/` is modified and needs to be integrated back into the template.  `./dev/diff.sh` will generate a diff file in `./target/demo.patch` if there are diffs with the `./target/demo_base` project.  We can use this diff to decide how to merge these changes back into the current template.

