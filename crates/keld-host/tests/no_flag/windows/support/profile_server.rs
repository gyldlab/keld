//! Profile renderer listener, request queue, stop/wake and worker-join owner.

use super::profile_observation::ProfileStateObservation;
use super::profile_response::{
    profile_state_response, renderer_request_path, write_profile_state_response,
};
use super::renderer::renderer_beacon_remaining;
use crate::RENDERER_ACCEPT_POLL;
use crate::windows_renderer_http::{
    PendingRendererRequest, RendererRequestRead, accept_renderer_connection,
    read_renderer_request_line,
};
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

pub(crate) struct ProfileStateServer {
    address: SocketAddr,
    observations: mpsc::Receiver<Result<ProfileStateObservation, String>>,
    commands: mpsc::Sender<ProfileStateCommand>,
    worker: Option<thread::JoinHandle<()>>,
}

enum ProfileStateCommand {
    Expect {
        case_name: String,
        deadline: Instant,
    },
    Stop,
}

impl ProfileStateServer {
    pub(crate) fn new() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind profile state server");
        let address = listener.local_addr().expect("profile state server address");
        listener
            .set_nonblocking(true)
            .expect("nonblocking profile state server");
        let (observed_tx, observations) = mpsc::channel();
        let (commands, command_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            if let Err(error) = serve_profile_state_loop(&listener, &command_rx, &observed_tx) {
                let _ = observed_tx.send(Err(error));
            }
        });
        Self {
            address,
            observations,
            commands,
            worker: Some(worker),
        }
    }

    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(crate) fn expect_case(&self, case_name: &str, deadline: Instant) {
        self.commands
            .send(ProfileStateCommand::Expect {
                case_name: case_name.to_owned(),
                deadline,
            })
            .expect("arm profile state case");
    }

    pub(crate) fn wait_for_case(
        &self,
        expected: &str,
        deadline: Instant,
    ) -> ProfileStateObservation {
        let remaining = renderer_beacon_remaining(deadline, Instant::now())
            .expect("profile state deadline remains");
        let observation = self
            .observations
            .recv_timeout(remaining)
            .expect("profile state report")
            .unwrap_or_else(|error| panic!("profile state server failed: {error}"));
        assert_eq!(observation.case_name, expected, "profile state case order");
        observation
    }
}

impl Drop for ProfileStateServer {
    fn drop(&mut self) {
        let _ = self.commands.send(ProfileStateCommand::Stop);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve_profile_state_loop(
    listener: &TcpListener,
    commands: &mpsc::Receiver<ProfileStateCommand>,
    observations: &mpsc::Sender<Result<ProfileStateObservation, String>>,
) -> Result<(), String> {
    let mut pending = Vec::<PendingRendererRequest>::new();
    let mut expected: Option<(String, Instant)> = None;
    let mut last_observation = None;
    loop {
        if expected.is_none() {
            match commands.recv_timeout(RENDERER_ACCEPT_POLL) {
                Ok(ProfileStateCommand::Expect {
                    case_name,
                    deadline,
                }) => expected = Some((case_name, deadline)),
                Ok(ProfileStateCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok(());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }
        match commands.try_recv() {
            Ok(ProfileStateCommand::Stop) | Err(mpsc::TryRecvError::Disconnected) => {
                return Ok(());
            }
            Ok(ProfileStateCommand::Expect { case_name, .. }) => {
                return Err(format!(
                    "profile state case `{case_name}` armed before the prior case completed"
                ));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let (expected_case, deadline) = expected.as_ref().expect("armed profile state case");
        let remaining = renderer_beacon_remaining(*deadline, Instant::now())?;

        let mut index = 0;
        while index < pending.len() {
            let request = {
                let PendingRendererRequest { stream, request } = &mut pending[index];
                read_renderer_request_line(request, |buffer| stream.read(buffer))
            }?;
            match request {
                RendererRequestRead::Pending => index += 1,
                RendererRequestRead::Empty => {
                    pending.swap_remove(index);
                }
                RendererRequestRead::Complete(request) => {
                    let mut matched = pending.swap_remove(index);
                    let path = renderer_request_path(&request)?;
                    let response =
                        profile_state_response(path, expected_case, last_observation.as_ref())?;
                    matched.stream.set_nonblocking(false).map_err(|error| {
                        format!("set profile state reply stream blocking: {error}")
                    })?;
                    matched
                        .stream
                        .set_write_timeout(Some(remaining))
                        .map_err(|error| format!("set profile state reply deadline: {error}"))?;
                    write_profile_state_response(&mut matched.stream, &response)?;
                    if let Some(observation) = response.observation {
                        last_observation = Some(observation.clone());
                        observations
                            .send(Ok(observation))
                            .map_err(|_| "profile state observation owner ended".to_owned())?;
                        expected = None;
                        break;
                    }
                }
            }
        }

        if accept_renderer_connection(listener, &mut pending, "profile state server")? {
            continue;
        }

        thread::park_timeout(remaining.min(RENDERER_ACCEPT_POLL));
    }
}
