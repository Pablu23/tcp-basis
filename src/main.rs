use std::io::prelude::*;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::{env, str, thread, time};

use anyhow::Result;

struct Client {
    id: u64,
    stream: TcpStream,
}

impl Client {
    fn new(id: u64, stream: TcpStream) -> Self {
        Client { id, stream }
    }
}

enum EventAction {
    Connected,
    Disconnected,
    MessageReceived(String),
}

struct Event {
    action: EventAction,
    client_id: u64,
}

struct Server {
    // listener: TcpListener,
    // server_handle: Option<JoinHandle<()>>,
    clients: Arc<Mutex<Vec<Client>>>,
    ip_address: String,
    highest_id: Arc<u64>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}

impl Server {
    pub fn new(ip_address: String) -> Self {
        let vec = vec![];
        let mutex = Mutex::new(vec);
        let arc = Arc::new(mutex);
        let highest = Arc::new(0u64);

        let (tx, rx) = mpsc::channel();

        Server {
            ip_address,
            clients: arc,
            highest_id: highest,
            // server_handle: None,
            sender: tx,
            receiver: rx,
        }
    }

    pub fn serve(&mut self) -> Result<()> {
        // self.server_handle = Some(self.spawn_listen_thread()?);
        self.spawn_listen_thread()?;
        self.spawn_health_check_thread()?;

        Ok(())
    }

    // pub fn shutdown(&mut self) -> Result<()> {
    //     let mut guard = self.clients.lock().unwrap();
    //     let clients = &mut *guard;
    //     for client in clients {
    //         client.stream.shutdown(Shutdown::Both)?;
    //     }

    //     Ok(())
    // }

    fn spawn_listen_thread(&self) -> Result<JoinHandle<()>> {
        let listener = TcpListener::bind(&self.ip_address)?;
        let clients = self.clients.clone();
        let mut highest = self.highest_id.clone();
        let sender = self.sender.clone();

        Ok(thread::spawn(move || {
            for stream in listener.incoming() {
                println!("Client connecting");
                highest = highest.checked_add(1).unwrap().into();
                println!("New Highest: {highest}");

                let mut guard = clients.lock().unwrap();
                let clients = &mut *guard;
                clients.push(Client::new(*highest, stream.unwrap()));
                sender
                    .send(Event {
                        action: EventAction::Connected,
                        client_id: *highest,
                    })
                    .unwrap();
                drop(guard);
                println!("Client connected");
            }
        }))
    }

    fn spawn_health_check_thread(&self) -> Result<JoinHandle<()>> {
        let clients = self.clients.clone();
        let sender = self.sender.clone();

        Ok(thread::spawn(move || loop {
            println!("HEALTH: Sleeping 10 sec");
            thread::sleep(time::Duration::from_secs(10));

            println!("HEALTH: Wakeup get Mutex");
            let mut guard = clients.lock().unwrap();
            let clients = &mut *guard;

            println!("HEALTH: Check Health");
            for client in clients {
                println!("HEALTH: Checking Client {}", client.id);
                // let health = client.stream.write(String::from("Alive?").as_bytes());
                //let health =
                let _ = Server::send_message(client, &String::from("Alive?"));
                let buffer = &mut [0u8; 6];
                let answer_result = client.stream.read(buffer);

                let t = match answer_result {
                    Ok(_) => {
                        if let Ok(answer) = str::from_utf8(buffer) {
                            if answer == "Alive!" {
                                Ok(())
                            } else {
                                Err(())
                            }
                        } else {
                            Err(())
                        }
                    }
                    Err(_) => Err(()),
                };

                if t.is_err() {
                    sender
                        .send(Event {
                            action: EventAction::Disconnected,
                            client_id: client.id,
                        })
                        .unwrap();
                }
            }
            println!("HEALTH: Dropping Mutex");
            drop(guard);
        }))
    }

    pub fn event_loop(&self) -> Result<()> {
        loop {
            let event = self.receiver.recv()?;

            // println!("Received Event");

            let mut guard = self.clients.lock().unwrap();
            let clients = &mut *guard;
            let client_pos = clients
                .iter()
                .position(|x| x.id == event.client_id)
                .unwrap();

            let client = clients.get_mut(client_pos).unwrap();

            match event.action {
                EventAction::Connected => {
                    Server::send_message(client, &String::from("Hello new Client")).unwrap();
                    let id = client.id;
                    Server::broadcast_message(
                        clients,
                        &format!("New Client Connected: Id {id}"),
                        Some(id),
                    )?;
                }
                EventAction::Disconnected => {
                    println!("EVENT: Client Id: {} disconnected", event.client_id);
                    let id = client.id;
                    Server::broadcast_message(
                        clients,
                        &format!("Client Disconnected: Id {id}"),
                        Some(id),
                    )?;
                    clients.swap_remove(client_pos);
                    ()
                }
                EventAction::MessageReceived(_) => todo!(),
            }

            drop(guard);
        }
    }

    pub fn send_message(client: &mut Client, msg: &String) -> Result<()> {
        let message_len = msg.as_bytes().len();
        let buf: [u8; 4] = u32::try_from(message_len)?.to_le_bytes();

        client.stream.write(&buf)?;

        client.stream.write(msg.as_bytes())?;

        println!("Send Message \"{msg}\" to {}", client.id);

        Ok(())
    }

    pub fn broadcast_message(
        clients: &mut Vec<Client>,
        message: &String,
        exclude: Option<u64>,
    ) -> Result<()> {
        // let mut guard = self.clients.lock().unwrap();
        // let clients = &mut *guard;
        for client in clients {
            match exclude {
                Some(exclude) => {
                    if exclude != client.id {
                        Server::send_message(client, message)?;
                    }
                }
                None => {
                    Server::send_message(client, message)?;
                }
            }
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        client(&String::from("alive"))?;
    } else {
        let mode = &args[1];

        match mode.as_str() {
            "server" => server(),
            "client" => client(&args[2]),
            _ => panic!("Wrong Arg"),
        }?;
    }

    Ok(())
}

fn server() -> Result<()> {
    let mut server = Server::new("127.0.0.1:1337".into());

    server.serve()?;

    server.event_loop()?;

    Ok(())
}

// fn handle_client(mut stream: TcpStream) -> Result<()> {
//     stream.write(b"Hello World!")?;
//     println!("Send Hello");
//     Ok(())
// }

fn client(name: &String) -> Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:1337")?;

    println!("Connected");
    let mut buffer = [0u8; 128];
    let mut msg_length_buf = [0u8; 4];
    let mut msg_length: u32 = 0;
    let mut content: String = String::new();

    let mut full_message_rcvd = true;

    loop {
        let read: usize;
        if full_message_rcvd {
            read = stream.read(&mut msg_length_buf)?;
            msg_length = u32::from_le_bytes(msg_length_buf);
            full_message_rcvd = false;
        } else {
            read = stream.read(&mut buffer)?;
            msg_length -= u32::try_from(read)?;
        }

        if read == 0 {
            break;
        }

        content += str::from_utf8(&buffer)?.trim_matches(char::from(0));

        if msg_length == 0 {
            if content == String::from("Alive?") {
                stream.write(String::from("Alive!").as_bytes())?;
            } else {
                println!("Message Received: {content}");
            }
            content = String::new();
            buffer = [0u8; 128];
            full_message_rcvd = true;

            if name == &String::from("disconnect") {
                stream.shutdown(Shutdown::Both)?;
                break;
            }
        }
    }

    println!("Disconnected");

    Ok(())
}
