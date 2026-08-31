#!/usr/bin/env bash
set -euo pipefail

# Official Mermaid CLI 11.16.0 multi-platform image, pinned by OCI index digest.
# Source: https://github.com/mermaid-js/mermaid-cli/pkgs/container/mermaid-cli%2Fmermaid-cli/985412512?tag=11.16.0
readonly MERMAID_IMAGE='ghcr.io/mermaid-js/mermaid-cli/mermaid-cli@sha256:29077c6bd02f14bdfdd5fee552d9c00fe68d4fab3cd84952d21e2d1faf2fadaf'

command -v git >/dev/null 2>&1 || {
  echo 'KELD-DOCS006: `git` is required. Install Git, then rerun `just mermaid-render-check`.' >&2
  exit 1
}
command -v docker >/dev/null 2>&1 || {
  echo 'KELD-DOCS006: `docker` is required for the pinned Mermaid renderer. Install/start Docker, then rerun `just mermaid-render-check`.' >&2
  exit 1
}
command -v perl >/dev/null 2>&1 || {
  echo 'KELD-DOCS006: `perl` is required for SVG accessibility checks. Install Perl, then rerun `just mermaid-render-check`.' >&2
  exit 1
}

run_with_timeout() {
  local seconds=$1
  shift
  perl -e '
    use strict;
    use warnings;
    my $seconds = shift @ARGV;
    my $pid = fork();
    die "fork failed: $!\n" unless defined $pid;
    if ($pid == 0) { exec @ARGV or exit 127; }
    $SIG{ALRM} = sub {
      kill "TERM", $pid;
      select undef, undef, undef, 1.0;
      kill "KILL", $pid;
      waitpid($pid, 0);
      exit 124;
    };
    alarm $seconds;
    waitpid($pid, 0);
    alarm 0;
    my $status = $?;
    exit(($status & 127) ? 128 + ($status & 127) : ($status >> 8));
  ' "$seconds" "$@"
}

# Git Bash rewrites Unix-looking arguments passed to native Windows programs.
# Docker bind sources need native host paths, while container paths must remain
# literal Linux paths. Convert the former explicitly so the docker-run subshell
# can disable MSYS argument conversion without making host mounts ambiguous.
running_under_msys() {
  case "${MSYSTEM:-}" in
    MINGW* | MSYS*) return 0 ;;
  esac
  case "$(uname -s 2>/dev/null || true)" in
    MINGW* | MSYS*) return 0 ;;
  esac
  return 1
}

docker_host_path() {
  local path=$1
  if running_under_msys; then
    command -v cygpath >/dev/null 2>&1 || {
      echo 'KELD-DOCS006: `cygpath` is required when rendering from Git Bash. Repair Git for Windows, then rerun.' >&2
      return 1
    }
    cygpath -am "$path"
  else
    printf '%s\n' "$path"
  fi
}

# Docker Desktop presents Git-Bash /tmp binds to Linux containers as root-owned
# even when the host directory belongs to the invoking Windows user. The
# renderer deliberately runs as the non-root Git-Bash uid/gid, so grant that
# isolated random output directory write access on MSYS only. Unix permissions
# remain the mktemp default; source and config mounts remain read-only.
prepare_docker_output_dir() {
  local path=$1
  if running_under_msys; then
    chmod 0777 -- "$path" || {
      echo "KELD-DOCS006: cannot make Docker output directory writable from Git Bash: '$path'. Repair its Windows ACL, then rerun." >&2
      return 1
    }
  fi
}

docker info >/dev/null 2>&1 || {
  echo 'KELD-DOCS006: Docker daemon is unavailable. Start Docker, then rerun `just mermaid-render-check`.' >&2
  exit 1
}
docker_host=$(docker context inspect --format '{{.Endpoints.docker.Host}}' 2>/dev/null) || {
  echo 'KELD-DOCS006: cannot resolve the active Docker endpoint. Fix the Docker context, then rerun.' >&2
  exit 1
}
readonly docker_host

workspace=$(git rev-parse --show-toplevel 2>/dev/null) || {
  echo 'KELD-DOCS006: not inside the Keld Git checkout. Change to the repository root, then rerun `just mermaid-render-check`.' >&2
  exit 1
}
readonly workspace
readonly render_config="$workspace/tools/mermaid-render-config.json"
[[ -f "$render_config" ]] || {
  echo 'KELD-DOCS006: `tools/mermaid-render-config.json` is missing. Restore it, then rerun `just mermaid-render-check`.' >&2
  exit 1
}

files=()
while IFS= read -r -d '' file; do
  files+=("$file")
done < <(git -C "$workspace" grep -Ilzi 'mermaid' -- '*.md' || true)

if [[ -f "$workspace/ROADMAP.md" ]]; then
  files+=("ROADMAP.md")
fi
if git -C "$workspace/docs/research" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  while IFS= read -r -d '' file; do
    files+=("docs/research/$file")
  done < <(git -C "$workspace/docs/research" grep -Ilzi 'mermaid' -- '*.md' || true)
fi

if [[ ${#files[@]} -eq 0 ]]; then
  echo 'KELD-DOCS006: no tracked Mermaid Markdown blocks were found. Restore the expected documentation or remove this gate in a reviewed policy change.' >&2
  exit 1
fi

render_dir=''
active_container=''
render_succeeded=0
keep_output=${KELD_MERMAID_KEEP_OUTPUT:-0}
[[ "$keep_output" == 0 || "$keep_output" == 1 ]] || {
  echo 'KELD-DOCS006: KELD_MERMAID_KEEP_OUTPUT must be 0 or 1. Correct it, then rerun.' >&2
  exit 1
}
docker_config_dir=$(mktemp -d /tmp/keld-mermaid-docker-config.XXXXXX)
readonly docker_config_dir
export DOCKER_HOST=$docker_host
export DOCKER_CONFIG=$docker_config_dir

cleanup() {
  if [[ -n "$active_container" ]]; then
    docker rm --force "$active_container" >/dev/null 2>&1 || true
  fi
  if [[ "$render_succeeded" == 1 && "$keep_output" == 0 && -n "$render_dir" ]]; then
    case "$render_dir" in
      /tmp/keld-mermaid-render.*) rm -rf -- "$render_dir" ;;
      *)
        echo "KELD-DOCS006: refused to clean unexpected render path '$render_dir'. Remove it manually after inspection." >&2
        ;;
    esac
  fi
  rmdir "$docker_config_dir" >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP

render_dir=$(mktemp -d /tmp/keld-mermaid-render.XXXXXX)
readonly render_dir
prepare_docker_output_dir "$render_dir"
docker_render_dir=$(docker_host_path "$render_dir")
readonly docker_render_dir
docker_render_config=$(docker_host_path "$render_config")
readonly docker_render_config
expected=0
render_index=0

if ! docker image inspect "$MERMAID_IMAGE" >/dev/null 2>&1; then
  if ! run_with_timeout 300 docker pull "$MERMAID_IMAGE"; then
    echo 'KELD-DOCS006: failed to pull the digest-pinned Mermaid image within 300 seconds. Check registry access and disk space, then rerun.' >&2
    exit 1
  fi
fi
docker image inspect "$MERMAID_IMAGE" >/dev/null 2>&1 || {
  echo 'KELD-DOCS006: the digest-pinned Mermaid image is unavailable after pull. Inspect Docker storage, then rerun.' >&2
  exit 1
}

for file in "${files[@]}"; do
  [[ -f "$workspace/$file" && ! -L "$workspace/$file" ]] || {
    echo "KELD-DOCS006: Mermaid source '$file' is missing, not regular, or a symlink. Use a tracked regular Markdown file, then rerun." >&2
    exit 1
  }
  block_count=$(grep -c '^```mermaid$' "$workspace/$file" || true)
  expected=$((expected + block_count))
  render_index=$((render_index + 1))
  safe_name=$(printf '%03d_%s' "$render_index" "${file//\//_}")
  container_name="keld-mermaid-$$-${render_index}"
  active_container=$container_name
  docker_source=$(docker_host_path "$workspace/$file")

  if ! (
    # These variables affect only the native docker.exe invocation. Host paths
    # above are already native/mixed; container paths below must not be rewritten.
    export MSYS_NO_PATHCONV=1
    export MSYS2_ARG_CONV_EXCL='*'
    run_with_timeout 120 docker run --rm \
      --name "$container_name" \
      --pull never \
      --network none \
      --read-only \
      --cap-drop ALL \
      --security-opt no-new-privileges \
      --cpus 2 \
      --memory 2g \
      --memory-swap 2g \
      --pids-limit 256 \
      --shm-size 256m \
      --tmpfs /tmp:rw,nosuid,nodev,noexec,size=512m \
      --env HOME=/tmp \
      --user "$(id -u):$(id -g)" \
      --volume "$docker_source:/input/source.md:ro" \
      --volume "$docker_render_dir:/out" \
      --volume "$docker_render_config:/config/mermaid.json:ro" \
      "$MERMAID_IMAGE" \
      --configFile /config/mermaid.json \
      --input /input/source.md \
      --output "/out/$safe_name.md" \
      --artefacts "/out/$safe_name-assets" \
      --jobs 2 \
      --quiet
  ); then
    echo "KELD-DOCS006: pinned Mermaid render failed or exceeded 120 seconds for file '$file'. Inspect the source and rerun 'just mermaid-render-check'." >&2
    exit 1
  fi
  active_container=''
done

actual=$(find "$render_dir" -type f -name '*.svg' -size +0 | wc -l | tr -d ' ')
if [[ "$actual" -ne "$expected" ]]; then
  echo "KELD-DOCS006: rendered $actual non-empty SVG files for $expected Mermaid blocks. Inspect $render_dir and fix the missing render." >&2
  exit 1
fi

while IFS= read -r svg; do
  perl -0777 -ne 'exit((/<title[^>]*>\s*[^<\s][^<]*<\/title>/s && /<desc[^>]*>\s*[^<\s][^<]*<\/desc>/s && !/<script\b|javascript:/i) ? 0 : 1)' "$svg" || {
    echo "KELD-DOCS006: rendered SVG '$svg' lacks a non-empty title/description or contains active script. Fix the source block and rerun." >&2
    exit 1
  }
done < <(find "$render_dir" -type f -name '*.svg' -print)

render_succeeded=1
if [[ "$keep_output" == 1 ]]; then
  output_note="SVG output retained in $render_dir"
else
  output_note='temporary SVG output validated and cleaned'
fi
echo "mermaid-render ok: $actual diagram(s), official CLI 11.16.0 image sha256:29077c6bd02f14bdfdd5fee552d9c00fe68d4fab3cd84952d21e2d1faf2fadaf, $output_note"
