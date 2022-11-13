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

                let sender = sender.clone();

                tokio::spawn(async move {
                    let mut message = String::new();

                    match reader.read_line(&mut message).await {
                        Ok(_) => {}
                        Err(_) => return
                    }

                    message.pop();

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


#[derive(Debug)]
pub enum ConsoleSendError {
    NotFound,
    PermissionDenied,
    OtherSocketClosed,
    GenericError(IOError)
}


impl From<IOError> for ConsoleSendError {
    fn from(e: IOError) -> Self {
        if let Some(n) = e.raw_os_error() && (n == 233 || n == 111) {
            return ConsoleSendError::OtherSocketClosed
        }

        match e.kind() {
            ErrorKind::PermissionDenied => ConsoleSendError::PermissionDenied,
            ErrorKind::NotFound => ConsoleSendError::NotFound,
            _ => ConsoleSendError::GenericError(e)
        }
    }
}


pub async fn send_message(bind_addr: &str, message: String) -> Result<String, ConsoleSendError> {
    let mut socket = LocalSocketStream::connect(bind_addr).await?;

    socket.write_all((message + "\n").as_bytes()).await?;
    socket.flush().await?;

    let mut msg = String::new();
    match socket.read_to_string(&mut msg).await {
        Ok(_) => {}
        Err(e) => match e.into() {
            ConsoleSendError::OtherSocketClosed => {}
            e => return Err(e)
        }
    }
    
    Ok(msg)
}