use futures::StreamExt;
use libp2p::{
    Multiaddr,
    NetworkBehaviour,
    PeerId,
    Transport,
    core::upgrade,
    identity,
    floodsub::{self, Floodsub, FloodsubEvent},
    mdns::{Mdns, MdnsEvent},
    mplex,
    noise,
    swarm::{NetworkBehaviourEventProcess, SwarmBuilder, SwarmEvent},
    // `TokioTcpConfig` is available through the `tcp-tokio` feature.
    tcp::TokioTcpConfig,
};
use std::error::Error;
use tokio::io::{self, AsyncBufReadExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create a random PeerId
    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());
    println!("Local peer id : {:?}", peer_id);

    // Create a keypair for authenticated encryption of the transport.
    let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
        .into_authentic(&id_keys)
        .expect("Signing libp2p-noise static DH Keypair failed.");

    //create a tokio-based TCP transport use noise for authenticated encryption and Mplex for
    // multiplexing of substreams on a TCP stream.
    let transport = TokioTcpConfig::new().nodelay(true)
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::NoiseConfig::xx(noise_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();

    //Create a Floodsub topic
    let floodsub_topic = floodsub::Topic::new("chat");

    // We create a custom network behaviour thatcombines floodsub and mDNS.
    // The derive generates a dleegating 'NetworkBehaviour' impl which in turn
    // requiers the implementations of 'NetworkBehaviourEventProcess' for
    // the events of each behaviour.
    #[derive(NetworkBehaviour)]
        struct MyBehaviour{
        floodsub: Floodsub,
        mdns: Mdns,
    }
    impl NetworkBehaviourEventProcess<FloodsubEvent> for MyBehaviour{
        //Called when 'floodsub' produces an event.
        fn inject_event(&mut self, message: FloodsubEvent){
            if let FloodsubEvent::Message(message) = message {
                println!( "Received: '{:?}' from {:?}", String::from_utf8_lossy(&message.data), message.source);
            }
        }
    }
    impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
        //called when 'mdns' produces an event.
        fn inject_event(&mut self, event: MdnsEvent){
            match event {
                MdnsEvent::Discovered(list) =>
                for (peer, _) in list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
                MdnsEvent::Expired(list) =>
                for (peer,_) in list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer)
                    }
                }
            }
        }
    }
    // Create a Swarm to manage peers and events.
    let mut swarm = {
        let mdns = Mdns::new(Default::default()).await?;
        let mut behaviour = MyBehaviour {
            floodsub: Floodsub::new(peer_id.clone()),
            mdns,
        };

        behaviour.floodsub.subscribe(floodsub_topic.clone());

        SwarmBuilder::new(transport, behaviour, peer_id)
            // We want the connection background tasks to be spawned
            // onto the tokio runtime.
            .executor(Box::new(|fut| { tokio::spawn(fut); }))
            .build()
    };

    //Reach out to another node if specified
    if let Some(to_dial) = std::env::args().nth(1) {
        let addr: Multiaddr = to_dial.parse()?;
        swarm.dial_addr(addr)?;
        println!("Dialed {:?}", to_dial)
    }

    //Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    //listen on all interfaces and whatever port the OS assigns
    swarm.listen_on("ip4/0.0.0.0/tcp/0".parse()?)?;

    // Kick it off
    loop {
        tokio::select! {
            line = stdin.next_line() => {
                let line = line?.expect("stdin closed");
                swarm.behaviour_mut().floodsub.publish(floodsub_topic.clone(), line.as_bytes());
            }
            event = swarm.select_next_some() => {
                if let SwarmEvent::NewListenAddr { address, .. } = event {
                    println!("Listening on {:?}", address);
                }
            }
        }
    }
}
