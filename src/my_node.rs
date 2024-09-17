use std::net::{TcpListener, TcpStream, SocketAddr};
use crate::domain::environment::Environment;
use crate::streamlet::Streamlet;


pub struct NodeSocket {
    id: u32,
    socket: TcpStream,
}

pub struct ServerSocket {
    address: SocketAddr,
    socket: TcpStream,
}

pub struct MyNode {
    environment: Environment,
    listener: TcpListener,
    node_sockets: Vec<NodeSocket>,
    server_sockets: Vec<ServerSocket>,
}

impl MyNode {

    pub fn new(environment: Environment) -> MyNode {
        let address = format!("{}:{}", environment.my_node.host, environment.my_node.port);
        let listener = TcpListener::bind(address).expect("Failed to bind to address");
        MyNode {
            environment,
            listener,
            node_sockets: Vec::new(),
            server_sockets: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        self.start_node_sockets();
        self.accept_loop();
        let mut streamlet = Streamlet::new(self.environment.my_node.id, self.environment.nodes.len() as u32, &self.node_sockets, &self.server_sockets);
        streamlet.start_protocol();
    }

    fn start_node_sockets(&mut self) {
        for node in &self.environment.nodes {
            let address = format!("{}:{}", node.host, node.port);
            match TcpStream::connect(address) {
                Ok(socket) => self.node_sockets.push(NodeSocket { id: node.id, socket }),
                Err(e) => eprintln!("[Node {}] Failed to connect to node {}: {}", self.environment.my_node.id, node.id, e),
            }
        }
    }

    fn accept_loop(&mut self) {
        let mut accepted = 0;
        let nodes_count = self.environment.nodes.len();
        while accepted < nodes_count {
            match self.listener.accept() {
                Ok((socket, addr)) => self.server_sockets.push(ServerSocket { address: addr, socket }),
                Err(e) => eprintln!("[Node {}] Failed to accept a connection: {}", self.environment.my_node.id, e)
            }
            accepted += 1;
        }
    }
}
