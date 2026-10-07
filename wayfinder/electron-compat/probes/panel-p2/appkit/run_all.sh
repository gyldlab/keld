#!/bin/bash
# Runs every scenario N times; transcripts/<scenario>.run<k>.jsonl, exit codes in transcripts/_runs.tsv
set -u
cd "$(dirname "$0")"
N=${N:-5}
SCEN="s1_close_veto_then_allow s2a_quit_two_dirty_allow s2b_quit_second_vetoes s2c_quit_first_vetoes_stop s2d_quit_first_vetoes_continue s3a_reentrancy_modal_from_gcd_block s3b_reentrancy_modal_from_runloop_block s3c_reentrancy_terminate_later s3d_terminate_later_default_mode_delivery n1_sync_yes_while_dirty n2_timeout_autoclose p3_second_close_during_modal_gated n3_second_close_during_modal_ungated"
mkdir -p transcripts
printf "scenario\trun\texit\n" > transcripts/_runs.tsv
for s in $SCEN; do
  for k in $(seq 1 "$N"); do
    ./harness "$s" "$k" "transcripts/$s.run$k.jsonl"
    printf "%s\t%s\t%s\n" "$s" "$k" "$?" >> transcripts/_runs.tsv
  done
done
