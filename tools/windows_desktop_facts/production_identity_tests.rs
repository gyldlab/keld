#[cfg(test)]
mod tests {
    use super::super::DesktopSampler;
    use super::Target;

    fn target(target_id: u32) -> Target {
        Target {
            adapter_high: -1,
            adapter_low: 17,
            target_id,
            device_path: format!(r"\\?\display#{}#a", "x".repeat(96)),
        }
    }

    fn history(sampler: &DesktopSampler) -> Vec<(i32, u32, u32, String)> {
        sampler
            .known
            .iter()
            .map(|target| {
                (
                    target.adapter_high,
                    target.adapter_low,
                    target.target_id,
                    target.device_path.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn desktop_identity_uses_each_complete_target_field() {
        let alternatives = [
            (
                "signed adapter high",
                Target {
                    adapter_high: 1,
                    ..target(7)
                },
            ),
            (
                "adapter low",
                Target {
                    adapter_low: 18,
                    ..target(7)
                },
            ),
            ("target ID", target(8)),
            (
                "complete device path",
                Target {
                    // The shared prefix extends beyond short device-name buffers.
                    device_path: format!(r"\\?\display#{}#b", "x".repeat(96)),
                    ..target(7)
                },
            ),
        ];

        for (field, alternative) in alternatives {
            let mut sampler = DesktopSampler::default();
            assert_eq!(sampler.display_id(target(7)).unwrap(), 1);
            assert_eq!(sampler.display_id(alternative).unwrap(), 2, "{field}");
            assert_eq!(sampler.display_id(target(7)).unwrap(), 1, "{field}");
            assert_eq!(sampler.known.len(), 2, "{field}");
        }
    }

    #[test]
    fn desktop_identity_retains_history_across_intervening_targets() {
        let mut sampler = DesktopSampler::default();
        assert_eq!(sampler.display_id(target(1)).unwrap(), 1);

        // Target 1 is absent from these allocator inputs. This does not simulate
        // native disconnection or a failed capture.
        for target_id in 2..=8 {
            assert_eq!(sampler.display_id(target(target_id)).unwrap(), target_id);
        }
        for target_id in [8, 3, 7, 2, 6, 4, 5] {
            assert_eq!(sampler.display_id(target(target_id)).unwrap(), target_id);
        }

        assert_eq!(sampler.known.len(), 8);
        assert_eq!(sampler.display_id(target(1)).unwrap(), 1);
        assert_eq!(sampler.known.len(), 8);
        assert_eq!(sampler.display_id(target(9)).unwrap(), 9);
    }

    #[test]
    fn desktop_identity_refuses_thirty_third_target_without_mutation() {
        let mut sampler = DesktopSampler::default();
        for target_id in 1..=32 {
            assert_eq!(sampler.display_id(target(target_id)).unwrap(), target_id);
        }
        assert_eq!(sampler.known.len(), 32);
        let before = history(&sampler);

        assert!(
            sampler.display_id(target(33)).is_err(),
            "a thirty-third distinct identity must be refused"
        );
        assert_eq!(history(&sampler), before);

        // Capacity refusal must not make already known identities unavailable.
        for target_id in (1..=32).rev() {
            assert_eq!(sampler.display_id(target(target_id)).unwrap(), target_id);
        }
        assert_eq!(history(&sampler), before);
    }
}
