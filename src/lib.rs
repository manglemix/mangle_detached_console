use interprocess::local_socket::tokio::{LocalSocketListener, LocalSocketStream, OwnedWriteHalf};
use tokio::{sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender}, select};
use std::{io::{Error as IOError, ErrorKind}, mem::take, path::Path, fs::remove_file, ffi::OsStr};
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
    receiver: UnboundedReceiver<Result<ReceiveEvent, IOError>>,
    _alive_sender: UnboundedSender<()>
}


impl ConsoleServer {
    pub fn bind(bind_addr: &OsStr) -> Result<Self, IOError> {
        {
            let path = AsRef::<Path>::as_ref(bind_addr);
            if path.is_file() {
                remove_file(path)?;
            }
        }

        let server = LocalSocketListener::bind(bind_addr)?;
        let (sender, receiver) = unbounded_channel();
        let (_alive_sender, mut alive_receiver) = unbounded_channel();

        tokio::spawn(async move {
            loop {
                let (reader, writer) = select! {
                    res = server.accept() => {
                        match res {
                            Ok(x) => x.into_split(),
                            Err(e) => {
                                if sender.send(Err(e)).is_err() {
                                    return
                                }
                                continue
                            }
                        }
                    }
                    _ = alive_receiver.recv() => return
                };

                let mut reader = BufReader::new(reader);

                let sender = sender.clone();

                tokio::spawn(async move {
                    let mut message = String::new();

                    match reader.read_line(&mut message).await {
                        Ok(_) => {}
                        Err(e) => {
                            let _ = sender.send(Err(e));
                            return
                        }
                    }

                    // remove newline
                    message.pop();

                    if sender.send(Ok(ReceiveEvent {
                        message,
                        writer,
                    })).is_err() {
                        return
                    }
                });
            }
        });

        Ok(Self { receiver, _alive_sender })
    }

    pub async fn accept(&mut self) -> Result<ReceiveEvent, IOError> {
        self.receiver.recv().await.unwrap()
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
        if let Some(n) = e.raw_os_error() {
            match n {
                233 => return ConsoleSendError::OtherSocketClosed,
                111 => return ConsoleSendError::NotFound,
                _ => {}
            }
        }

        match e.kind() {
            ErrorKind::PermissionDenied => ConsoleSendError::PermissionDenied,
            ErrorKind::NotFound => ConsoleSendError::NotFound,
            _ => ConsoleSendError::GenericError(e)
        }
    }
}


pub async fn send_message(bind_addr: &OsStr, message: String) -> Result<String, ConsoleSendError> {
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