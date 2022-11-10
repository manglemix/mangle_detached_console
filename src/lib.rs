use interprocess::local_socket::tokio::{LocalSocketListener, LocalSocketStream, OwnedWriteHalf, OwnedReadHalf};
use tokio::{sync::mpsc::{unbounded_channel, UnboundedReceiver}, task::JoinHandle};
use std::{io::Error as IOError, mem::take, collections::HashSet};
use futures_lite::io::{AsyncReadExt, AsyncWriteExt};


pub struct ReceiveEvent {
    message: String,
    writer: OwnedWriteHalf
}


impl ReceiveEvent {
    pub fn take_message(&mut self) -> String {
        take(&mut self.message)
    }

    pub fn get_writer(&mut self) -> &mut OwnedWriteHalf {
        &mut self.writer
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
                let (mut reader, writer) = match server.accept().await {
                    Ok(x) => x.into_split(),
                    Err(_) => continue
                };

                let sender = sender.clone();

                tokio::spawn(async move {
                    let mut buffer = Vec::new();
    
                    match reader.read_to_end(&mut buffer).await {
                        Ok(_) => {}
                        Err(_) => return
                    }
    
                    let message = match String::from_utf8(buffer) {
                        Ok(x) => x,
                        Err(_) => return
                    };
    
                    let _ = sender.send(ReceiveEvent {
                        message,
                        writer: writer,
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





pub async fn send_message(bind_addr: &str, message: &str) -> Result<String, IOError> {
    let (mut reader, mut writer) = LocalSocketStream::connect(bind_addr).await?.into_split();
    writer.write_all(message.as_bytes()).await?;
    writer.close().await?;
    let mut msg = String::new();
    reader.read_to_string(&mut msg).await?;
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
            send_message(bind_addr, args.join(" ").to_string().as_str()).await
        )
    } else {
        InterceptResult::NoMatch(args)
    }
}
