use libp2p::{identity, PeerId, ping::{Ping, PingConfig}, swarm::{Swarm}};
use futures::executor::block_on;
use futures::prelude::*;
use std::{task::Poll, error::Error};


fn main() -> Result<(), Box<dyn Error>> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id : {:?}", local_peer_id);

    let transport = block_on(libp2p::development_transport(local_key))?;
    let behaviour = Ping::new(PingConfig::new().with_keep_alive(true));
    let mut swarm = Swarm::new(transport,behaviour,local_peer_id);
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    if let Some(addr) = std::env::args().nth(1) {
        let remote = addr.parse()?;
        swarm.dial_addr(remote)?;
        println!("Dialed {}", addr);
    }
    let mut listening = false;
    block_on(future::poll_fn(move |cx| loop {
        match swarm.poll_next_unpin(cx) {
            Poll::Ready(Some(event)) => println!("{:?}", event),
            Poll::Ready(None) => return Poll::Ready(()),
            Poll::Pending => {
                if !listening {
                for addr in Swarm::listeners(&swarm) {
            println!("Listening on {}", addr);
            listening = true;
            }
            }
                return Poll::Pending;
            }
        }
    }));
    Ok(())

}
