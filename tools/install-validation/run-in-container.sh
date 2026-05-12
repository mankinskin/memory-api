#!/usr/bin/env bash
set -euo pipefail

readonly CONTRACT_SLUG="memory-api/install-contracts/cli-and-viewer-installation"
readonly TOOL_NAMES=(rule spec ticket audit)

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
memory_api_root=$(cd -- "$script_dir/../.." && pwd)
work_root=/tmp/memory-api-install-validation
install_cargo_home=$work_root/cargo-home
install_target_dir=$work_root/target

log_step() {
    printf '\n[%s] %s\n' "$1" "$2"
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

find_install_contract_dir() {
    local dir

    for dir in "$memory_api_root/.spec/specs"/*; do
        [[ -f "$dir/spec.toml" ]] || continue
        if grep -Fq "slug = \"$CONTRACT_SLUG\"" "$dir/spec.toml"; then
            printf '%s\n' "$dir"
            return 0
        fi
    done

    fail "could not find install contract spec for $CONTRACT_SLUG"
}

contract_dir=$(find_install_contract_dir)
cli_matrix_path="$contract_dir/sections/cli-scenario-matrix.md"

scenario_row() {
    local scenario_id=$1
    local row

    row=$(grep -F "| $scenario_id |" "$cli_matrix_path" | head -n 1 || true)
    [[ -n "$row" ]] || fail "missing scenario row for $scenario_id"
    printf '%s\n' "$row"
}

scenario_commands_cell() {
    local scenario_id=$1

    scenario_row "$scenario_id" | cut -d'|' -f4
}

extract_backtick_tokens() {
    local text=$1

    while [[ "$text" =~ \`([^\`]*)\` ]]; do
        printf '%s\n' "${BASH_REMATCH[1]}"
        text=${text#*\`}
        text=${text#*\`}
    done
}

scenario_commands() {
    extract_backtick_tokens "$(scenario_commands_cell "$1")"
}

prepare_tool_install_env() {
    rm -rf "$install_cargo_home"
    mkdir -p "$install_cargo_home/bin" "$install_target_dir" "$work_root"
    export CARGO_HOME="$install_cargo_home"
    export CARGO_TARGET_DIR="$install_target_dir"
    export PATH="$install_cargo_home/bin:$PATH"
}

run_command() {
    local workdir=$1
    local command=$2

    printf '  $ %s\n' "$command"
    (
        cd "$workdir"
        bash -c "$command"
    )
}

assert_tools_available() {
    local tool

    for tool in "${TOOL_NAMES[@]}"; do
        [[ -x "$install_cargo_home/bin/$tool" ]] || fail "missing installed tool: $tool"
        "$install_cargo_home/bin/$tool" --help >/dev/null
    done
}

assert_tools_unavailable() {
    local tool

    for tool in "${TOOL_NAMES[@]}"; do
        if [[ -x "$install_cargo_home/bin/$tool" ]]; then
            fail "tool should have been uninstalled: $tool"
        fi
    done
}

install_tools() {
    local command

    log_step CLI-01 "install binaries into a clean Cargo home"
    prepare_tool_install_env
    while IFS= read -r command; do
        [[ -n "$command" ]] || continue
        run_command "$memory_api_root" "$command"
    done < <(scenario_commands CLI-01)
    assert_tools_available
}

uninstall_tools() {
    local command

    log_step CLI-02 "uninstall installed binaries cleanly"
    while IFS= read -r command; do
        [[ -n "$command" ]] || continue
        run_command "$work_root" "$command"
    done < <(scenario_commands CLI-02)
    assert_tools_unavailable
}

fresh_repo() {
    local name=$1
    local repo=$work_root/$name/repo

    rm -rf "$work_root/$name"
    mkdir -p "$repo"
    if command -v git >/dev/null; then
        git init -q "$repo"
    fi
    printf '%s\n' "$repo"
}

assert_root_layout() {
    local repo=$1
    local root_name

    for root_name in .rule .spec .ticket .audit; do
        [[ -d "$repo/$root_name" ]] || fail "missing $root_name in $repo"
        [[ -f "$repo/$root_name/.gitignore" ]] || fail "missing $root_name/.gitignore in $repo"
    done
}

run_cli_03() {
    local repo command

    log_step CLI-03 "initialize local roots from a fresh repository"
    repo=$(fresh_repo cli-03)
    while IFS= read -r command; do
        [[ -n "$command" ]] || continue
        run_command "$repo" "$command"
    done < <(scenario_commands CLI-03)
    assert_root_layout "$repo"
}

run_cli_04() {
    local repo nested_dir command

    log_step CLI-04 "discover local roots from a nested directory"
    repo=$(fresh_repo cli-04)
    while IFS= read -r command; do
        [[ -n "$command" ]] || continue
        run_command "$repo" "$command"
    done < <(scenario_commands CLI-03)

    nested_dir="$repo/apps/sandbox/src"
    mkdir -p "$nested_dir"
    while IFS= read -r command; do
        [[ -n "$command" ]] || continue
        run_command "$nested_dir" "$command"
    done < <(scenario_commands CLI-04)

    [[ ! -e "$nested_dir/.rule" ]] || fail "nested .rule root should not be created"
    [[ ! -e "$nested_dir/.spec" ]] || fail "nested .spec root should not be created"
    [[ ! -e "$nested_dir/.ticket" ]] || fail "nested .ticket root should not be created"
}

write_rule_target_fixture() {
    local repo=$1

    cat > "$repo/rule-targets.yaml" <<'EOF'
targets:
  - name: sandbox-readme
    repo_scope: sandbox
    file_kind: README
    section: overview
    output_path: generated/README.md
EOF
}

first_spec_id() {
    local repo=$1
    local spec_dir

    for spec_dir in "$repo/.spec/specs"/*; do
        [[ -d "$spec_dir" ]] || continue
        basename "$spec_dir"
        return 0
    done

    fail "expected one created spec in $repo/.spec/specs"
}

run_cli_05() {
    local repo spec_id command

    log_step CLI-05 "materialize canonical folders and run the documented smoke commands"
    repo=$(fresh_repo cli-05)
    write_rule_target_fixture "$repo"

    while IFS= read -r command; do
        [[ -n "$command" ]] || continue
        if [[ "$command" == *"<spec-id>"* ]]; then
            spec_id=$(first_spec_id "$repo")
            command=${command//<spec-id>/$spec_id}
        fi
        run_command "$repo" "$command"
    done < <(scenario_commands CLI-05)

    [[ -d "$repo/.rule/rules" ]] || fail "expected .rule/rules after first rule creation"
    [[ -d "$repo/.spec/specs" ]] || fail "expected .spec/specs after first spec creation"
    [[ -d "$repo/.ticket/tickets" ]] || fail "expected .ticket/tickets after first ticket creation"
    [[ -f "$repo/generated/README.md" ]] || fail "expected generated README output from sync-targets"
    grep -Fq "Install validation content." "$repo/generated/README.md" \
        || fail "generated README did not include the created rule body"
}

log_step setup "using $(rustc --version)"
install_tools
uninstall_tools
install_tools
run_cli_03
run_cli_04
run_cli_05
log_step done "all CLI install scenarios passed in Docker"
