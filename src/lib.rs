use interprocess::local_socket::tokio::{LocalSocketListener, LocalSocketStream, OwnedWriteHalf};
use tokio::{sync::mpsc::{unbounded_channel, UnboundedReceiver}, task::JoinHandle};
use std::{io::{Error as IOError, ErrorKind}, mem::take, collections::HashSet};
use futures::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, AsyncReadExt};


pub struct ReceiveEvent {
    message: String,
    writer: OwnedWriteHalf
}


impl ReceiveEvent {
    pub fn take_message(&mut self) -> String {
        take(&mut self.message)
    }

    pub async fn write_all(&mut self, message: &str) -> Result<(), IOError> {
        self.writer.write_all(message.as_bytes()).await
    }
}


pub struct ConsoleServer {
    receiver: UnboundedReceiver<ReceiveEvent>,
    handle: JoinHandle<()>
}


impl ConsoleServer {
    pub fn bind(bind_addr: &str) -> Result<Self, IOError> {
        let server = LocalSocketListener::bind(bind_addr)?;
        let (sender, receiver) = unbounded_channel();

        let handle = tokio::spawn(async move {
            loop {
                let (reader, writer) = match server.accept().await {
                    Ok(x) => x.into_split(),
                    Err(_) => continue
                };

                let mut reader = BufReader::new(reader);

                eprintln!("LMAO");
                let sender = sender.clone();

                tokio::spawn(async move {
                    let mut message = String::new();

                    match reader.read_line(&mut message).await {
                        Ok(_) => {}
                        Err(_) => return
                    }

                    message.pop();

                    eprintln!("LMAO2");

                    let _ = sender.send(ReceiveEvent {
                        message,
                        writer,
                    });
                });
            }
        });

        Ok(Self { receiver, handle })
    }

    pub async fn accept(&mut self) -> ReceiveEvent {
        self.receiver.recv().await.unwrap()
    }
}


impl Drop for ConsoleServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}


pub async fn send_message(bind_addr: &str, message: String) -> Result<String, IOError> {
    let mut socket = LocalSocketStream::connect(bind_addr).await?;

    socket.write_all((message + "\n").as_bytes()).await?;
    socket.flush().await?;

    let mut msg = String::new();
    match socket.read_to_string(&mut msg).await {
        Ok(_) => {}
        Err(e) => {
            if let Some(n) = e.raw_os_error() {
                if n != 233 {
                    return Err(e)
                }
            } else {
                return Err(e)
            }
        }
    }
    Ok(msg)
}


pub enum InterceptResult {
    NoMatch(Vec<String>),
    Matched(Result<String, IOError>)
}


pub async fn intercept_args(bind_addr: &str, commands_to_intercept: HashSet<&str>) -> InterceptResult {
    let args: Vec<String> = std::env::args().collect();
    
    if commands_to_intercept.contains(args.get(1).unwrap().as_str()) {
        InterceptResult::Matched(
            send_message(bind_addr, args.join(" ").to_string()).await
        )
    } else {
        InterceptResult::NoMatch(args)
    }
}
