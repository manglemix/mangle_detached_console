use interprocess::local_socket::tokio::{LocalSocketListener, LocalSocketStream, OwnedWriteHalf, OwnedReadHalf};
use tokio::{sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender}, task::JoinHandle};
use std::{io::Error as IOError, mem::take, collections::HashSet};
use futures_lite::io::{AsyncReadExt, AsyncWriteExt};


pub trait CommunicationState {
    fn new() -> Self;
}


impl CommunicationState for () {
    fn new() -> Self {
        ()
    }
}


pub struct ReceiveEvent<T: CommunicationState + Send + 'static> {
    message: String,
    sender: UnboundedSender<Self>,
    reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
    buf_size: usize,
    state: T
}


impl<T: CommunicationState + Send + 'static> ReceiveEvent<T> {
    pub fn deferred_read(mut self) {
        self.message = String::new();

        tokio::spawn(async move {
            let mut running_buffer = Vec::new();

            loop {
                let mut buffer = vec![0; self.buf_size];

                let read_size = match self.reader.read(buffer.as_mut_slice()).await {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break
                };

                running_buffer.extend_from_slice(buffer.split_at(read_size).0);

                let message = match String::from_utf8(running_buffer.clone()) {
                    Ok(x) => x,
                    Err(e) => continue
                };

                if !message.ends_with('\n') {
                    continue
                }

                let _ = self.sender.send(ReceiveEvent {
                    message,
                    sender: self.sender.clone(),
                    reader: self.reader,
                    writer: self.writer,
                    buf_size: self.buf_size,
                    state: self.state,
                });
                break
            }
        });
    }

    pub fn take_message(&mut self) -> String {
        take(&mut self.message)
    }

    pub fn get_writer(&mut self) -> &mut OwnedWriteHalf {
        &mut self.writer
    }
}


pub struct ConsoleServer<T: CommunicationState + Send + 'static> {
    receiver: UnboundedReceiver<ReceiveEvent<T>>,
    handle: JoinHandle<()>
}


impl<T: CommunicationState + Send + 'static> ConsoleServer<T> {
    pub fn bind(bind_addr: &str, buf_size: usize) -> Result<Self, IOError> {
        let server = LocalSocketListener::bind(bind_addr)?;
        let (sender, receiver) = unbounded_channel();

        let handle = tokio::spawn(async move {
            loop {
                let (reader, writer) = match server.accept().await {
                    Ok(x) => x.into_split(),
                    Err(_) => continue
                };

                ReceiveEvent {
                    message: String::new(),
                    sender: sender.clone(),
                    reader,
                    writer,
                    buf_size,
                    state: T::new()
                }.deferred_read();
            }
        });

        Ok(Self { receiver, handle })
    }

    pub async fn accept(&mut self) -> ReceiveEvent<T> {
        self.receiver.recv().await.unwrap()
    }
}


impl<T: CommunicationState + Send + 'static> Drop for ConsoleServer<T> {
    fn drop(&mut self) {
        self.handle.abort();
    }
}


pub struct ConsoleClient {
    pipe: LocalSocketStream
}


impl ConsoleClient {
    pub async fn connect(bind_addr: &str) -> Result<Self, IOError> {
        LocalSocketStream::connect(bind_addr).await.map(|pipe| Self { pipe })
    }

    pub async fn send(&mut self, message: &str) -> Result<(), IOError> {
        self.pipe.write_all(message.as_bytes()).await
    }
}


pub enum InterceptResult {
    NoMatch(Vec<String>),
    Matched(Result<(), IOError>)
}


pub async fn intercept_args(bind_addr: &str, commands_to_intercept: HashSet<&str>) -> InterceptResult {
    let args: Vec<String> = std::env::args().collect();
    
    if commands_to_intercept.contains(args.get(1).unwrap().as_str()) {
        InterceptResult::Matched(
            match ConsoleClient::connect(bind_addr).await {
                Ok(mut client) => {
                    client.send(args.join(" ").to_string().as_str()).await
                }
                Err(e) => Err(e)
            }
        )
    } else {
        InterceptResult::NoMatch(args)
    }
}
