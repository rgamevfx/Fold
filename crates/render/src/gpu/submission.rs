//! Admission, queue submission and completion publication.
use super::*;
use std::sync::atomic::Ordering;
impl Renderer {
    pub(super) fn submit_request(
        &mut self,
        encoded: encoding::Encoded,
        config: Option<&fold_color::Config>,
        cancel: &fold_media::Cancel,
        class: Option<crate::scheduling::Class>,
    ) -> Result<GpuFrame, fold_foundation::Failure> {
        let encoding::Encoded {
            mut encoder,
            leases,
            buffers,
            status,
            image,
            dependencies,
            retained,
            coverage_retained,
            mut stats,
            aces,
            evaluate_begin,
            uploads,
        } = encoded;
        cancel.check()?;
        self.host.check()?;
        status.encode(&mut encoder);
        let begin = std::time::Instant::now();
        let _permit = class
            .map(|class| crate::scheduling::Scheduler::shared().enter(class, cancel))
            .transpose()?;
        stats.graphics_queue_nanoseconds += begin.elapsed().as_nanos();
        cancel.check()?;
        self.host.check()?;
        let submit_begin = std::time::Instant::now();
        self.host.submit(encoder, leases, buffers);
        for upload in &uploads {
            upload.submitted();
        }
        drop(uploads);
        stats.queue_submit_nanoseconds += submit_begin.elapsed().as_nanos();
        let ready = status.submitted();
        for (key, image) in retained {
            self.host
                .sources()
                .insert(key, image, ready.clone(), self.host.memory().budget);
        }
        for value in coverage_retained {
            self.coverage
                .retain(value, ready.clone(), self.host.memory().budget);
        }
        stats.evaluate_cpu_nanoseconds = evaluate_begin.elapsed().as_nanos();
        cancel.check()?;
        self.host.check()?;
        Ok(GpuFrame {
            image,
            host: self.host.clone(),
            ready,
            dependencies,
            working_space: if aces {
                WorkingSpace::AcesCg
            } else {
                WorkingSpace::LegacyLinearSrgb
            },
            config_identity: config.map(|c| c.identity().content_sha256.clone()),
            timing: None,
            lease_id: host::NEXT_ID.fetch_add(1, Ordering::Relaxed),
            statistics: stats,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_media::{Cancel, Decoder};
    use std::time::{Duration, Instant};

    fn graph() -> RenderGraph {
        RenderGraph {
            width: 32,
            height: 32,
            nodes: vec![ImageOp::Solid {
                rgba: [0.2, 0.1, 0., 0.5],
            }],
            output: 0,
        }
    }
    fn encoded(renderer: &mut Renderer, cancel: &Cancel) -> encoding::Encoded {
        let mut decoder = Decoder::default();
        let request = renderer
            .prepare_request(graph(), None, &mut decoder, cancel, None)
            .unwrap();
        renderer
            .encode_request(request, None, &mut decoder, cancel)
            .unwrap()
    }
    fn released(host: &Host) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            // Even a stopped device must pump completion callbacks.
            let _ = host.poll();
            let memory = host.memory();
            if memory.allocated == memory.reusable {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "unreleased resources: {memory:?}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    #[ignore = "requires native Vulkan"]
    fn cancellation_before_submission_releases_encoded_leases() {
        let (host, _) = pollster::block_on(Host::headless(1024 * 1024)).unwrap();
        let mut renderer = Renderer::new(host.clone()).unwrap();
        let cancel = Cancel::default();
        let packet = encoded(&mut renderer, &cancel);
        let reservation = Arc::downgrade(&packet.buffers[0]);
        cancel.cancel();
        assert!(
            renderer
                .submit_request(packet, None, &cancel, None)
                .is_err()
        );
        assert!(reservation.upgrade().is_none());
        drop(renderer);
        released(&host);
    }
    #[test]
    #[ignore = "requires native Vulkan"]
    fn inflight_cancellation_and_device_failure_release_completion_owned_leases() {
        for destroy in [false, true] {
            let (host, _) = pollster::block_on(Host::headless(1024 * 1024)).unwrap();
            let mut renderer = Renderer::new(host.clone()).unwrap();
            let cancel = Cancel::default();
            let packet = encoded(&mut renderer, &cancel);
            let reservation = Arc::downgrade(&packet.buffers[0]);
            let mut frame = renderer
                .submit_request(packet, None, &cancel, None)
                .unwrap();
            cancel.cancel();
            assert!(frame.readback(&cancel).is_err());
            if destroy {
                host.device().destroy();
                let deadline = Instant::now() + Duration::from_secs(10);
                while host.check().is_ok() {
                    let _ = host.poll();
                    assert!(
                        Instant::now() < deadline,
                        "device-loss callback did not arrive"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(frame.is_ready().is_err());
            }
            drop(frame);
            drop(renderer);
            released(&host);
            assert!(reservation.upgrade().is_none());
        }
    }
    #[test]
    #[ignore = "requires native Vulkan"]
    fn preview_export_contention_cancellation_and_device_failure_release_admission() {
        use crate::scheduling::{Class, Scheduler};
        for destroy in [false, true] {
            let (host, _) = pollster::block_on(Host::headless(1024 * 1024)).unwrap();
            let scheduler = Scheduler::shared();
            let held = scheduler.enter(Class::Viewer, &Cancel::default()).unwrap();
            let cancel = Cancel::default();
            let token = cancel.clone();
            let worker_host = host.clone();
            let worker = std::thread::spawn(move || {
                let mut renderer = Renderer::new(worker_host).unwrap();
                renderer.evaluate_scheduled(
                    graph(),
                    None,
                    &mut Decoder::default(),
                    &token,
                    Class::Export,
                )
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            while scheduler.queue_depth().0 == 0 {
                assert!(!worker.is_finished() && Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(host.memory().allocated > host.memory().reusable);
            if destroy {
                host.device().destroy();
                let _ = host.poll();
            } else {
                cancel.cancel();
            }
            drop(held);
            assert!(worker.join().unwrap().is_err());
            assert_eq!(scheduler.queue_depth().0, 0);
            assert!(scheduler.enter(Class::Viewer, &Cancel::default()).is_ok());
            released(&host);
        }
    }
}
