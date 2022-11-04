use mangle_local_pipes::{PipeServer, LocalPipe};
use tokio::{sync::mpsc::{unbounded_channel, UnboundedReceiver}, io::{AsyncReadExt, AsyncWriteExt}};
use std::io::Error as IOError;


pub struct ConsoleServer {
    receiver: UnboundedReceiver<String>
}


impl ConsoleServer {
    pub async fn bind(bind_addr: String, buf_size: usize) -> Result<Self, IOError> {
        let mut server = PipeServer::bind(bind_addr).await?;
        let (sender, receiver) = unbounded_channel();

        tokio::spawn(async move {
            loop {
                let mut pipe = match server.accept().await {
                    Ok(x) => x,
                    Err(_) => continue
                };

                let new_sender = sender.clone();

                tokio::spawn(async move {
                    let mut buffer = vec![0; buf_size];

                    loop {
                        let read_size = match pipe.read(buffer.as_mut_slice()).await {
                            Ok(0) => break,
                            Ok(n) => n,
                            Err(_) => break
                        };

                        let message = match String::from_utf8(
                            buffer.split_at(read_size).0.into()
                        ) {
                            Ok(x) => x,
                            Err(_) => break
                        };

                        match new_sender.send(message) {
                            Ok(_) => {}
                            Err(_) => break
                        };
                    }
                });
            }
        });

        Ok(Self { receiver })
    }

    pub async fn read(&mut self) -> String {
        self.receiver.recv().await.unwrap()
    }
}


pub struct ConsoleClient {
    pipe: LocalPipe
}


impl ConsoleClient {
    pub fn connect(bind_addr: String) -> Result<Self, IOError> {
        mangle_local_pipes::connect(bind_addr).map(|pipe| Self { pipe })
    }

    pub async fn send(&mut self, message: &str) -> Result<(), IOError> {
        self.pipe.write_all(message.as_bytes()).await
    }
}


#[macro_export]
macro_rules! forward_args {
    (
        bind_addr: literal,
        disconnected_msg: literal,
        send_err_msg: literal,
        $($to_forward: literal,)*
    ) => {{
        let args: Vec<String> = env::args().collect();
        match args.get(0).unwrap() {
            $($to_forward |)* => {
                ConsoleClient::connect($bind_addr)
                    .expect(disconnected_msg)
                    .send(args.join(" ").as_str())
                    .expect(send_err_msg)
            }
            _ => {}
        }
    }};
}
