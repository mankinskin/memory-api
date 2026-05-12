# CLI Scenario Matrix

| ID | Scenario | Commands | Assertions |
| --- | --- | --- | --- |
| CLI-01 | Install binaries into a clean Cargo home | `cargo install --path tools/cli/rule-cli --bin rule`, `cargo install --path tools/cli/spec-cli --bin spec`, `cargo install --path tools/cli/ticket-cli --bin ticket`, `cargo install --path tools/cli/audit-cli --bin audit` | Installed binaries are on `PATH` and `--help` succeeds for each tool |
| CLI-02 | Deinstall binaries cleanly | `cargo uninstall rule-cli`, `cargo uninstall spec-cli`, `cargo uninstall ticket-cli`, `cargo uninstall audit-cli` | Installed binaries are no longer available after uninstall |
| CLI-03 | First-run root initialization without `--index-root` | `rule list`, `spec list`, `ticket board show`, `audit run .` from a fresh repo root | Commands succeed or initialize local roots as documented; folder-local `.gitignore` files exist |
| CLI-04 | Nested-directory root discovery | Run `rule list`, `spec list`, and `ticket board show` from a nested subdirectory after root init | Tools discover the nearest parent `.rule`, `.spec`, and `.ticket` roots |
| CLI-05 | Canonical folder materialization and local workflow smoke | `rule create --title "Install validation rule" --slug "sandbox/readme/install-validation" --file-kind README --section overview --body "Install validation content." --repo sandbox`, `spec create --title "Install validation spec" --slug "sandbox/install-validation" --component sandbox`, `ticket create --title "Install validation ticket" --type tracker-improvement`, `rule sync-targets --config rule-targets.yaml`, `spec refs <spec-id> validate`, `ticket board show`, `audit run .` | `rules/`, `specs/`, and `tickets/` appear on first content creation and the documented common-task commands remain valid |

## Notes

- CLI scenarios are the required first gating path for the Docker harness and CI.
- These scenarios are the minimum contract that the generated README install section must continue to advertise.
- CLI-05 assumes a harness-managed `rule-targets.yaml` fixture that targets `repo_scope: sandbox`, `file_kind: README`, and `section: overview`.
