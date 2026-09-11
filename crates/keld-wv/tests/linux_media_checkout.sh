#!/usr/bin/env bash

keld_path_is_within() {
  local candidate=$1
  local root=$2
  case "$candidate" in
    "$root"|"$root"/*) return 0 ;;
    *) return 1 ;;
  esac
}

keld_validate_evidence_root() {
  local candidate=$1
  local git_dir=$2
  local git_common_dir=$3
  ! keld_path_is_within "$candidate" "$git_dir" &&
    ! keld_path_is_within "$candidate" "$git_common_dir"
}

keld_checkout_is_clean() {
  local checkout_root=$1
  local evidence_relative=$2
  if ! git -C "$checkout_root" diff --quiet HEAD -- ||
    ! git -C "$checkout_root" diff --cached --quiet HEAD --; then
    return 1
  fi

  local untracked_path
  local -a pipeline_status
  git -C "$checkout_root" ls-files -z --others --exclude-standard |
    while IFS= read -r -d '' untracked_path; do
      if [ -n "$evidence_relative" ]; then
        case "$untracked_path" in
          "$evidence_relative"|"$evidence_relative"/*) continue ;;
        esac
      fi
      exit 1
    done
  pipeline_status=("${PIPESTATUS[@]}")
  [ "${pipeline_status[0]}" -eq 0 ] && [ "${pipeline_status[1]}" -eq 0 ]
}
