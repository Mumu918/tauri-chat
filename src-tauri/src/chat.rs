use libp2p::core::upgrade;
use libp2p::floodsub::{Floodsub, FloodsubEvent, Topic};
use libp2p::futures::StreamExt;
use libp2p::mdns::{Mdns, MdnsConfig, MdnsEvent};
use libp2p::ping::{Ping, PingConfig, PingEvent};
use libp2p::swarm::{NetworkBehaviourEventProcess, Swarm, SwarmBuilder, SwarmEvent};
use libp2p::tcp::{GenTcpConfig, TokioTcpTransport};
use libp2p::Transport;
use libp2p::{identity::Keypair, noise, yamux, NetworkBehaviour, PeerId};
use std::error::Error;
use tauri::{Manager, Window};
use tokio::sync::oneshot;

#[derive(NetworkBehaviour)]
#[behaviour(event_process = true)]
struct MyBehaviour {
    mdns: Mdns,
    ping: Ping,
    floodsub: Floodsub,

    #[behaviour(ignore)]
    window: Window,
}
impl MyBehaviour {
    async fn new(peer_id: PeerId, window: Window) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            mdns: Mdns::new(MdnsConfig::default()).await?,
            ping: Ping::new(PingConfig::default().with_keep_alive(true)),
            floodsub: Floodsub::new(peer_id),
            window: window,
        })
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer_id, multiaddr) in list {
                    self.floodsub.add_node_to_partial_view(peer_id);
                    println!("在网络中加入节点: {:?} {:?}", peer_id, multiaddr);
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer_id, multiaddr) in list {
                    if !self.mdns.has_node(&peer_id) {
                        self.floodsub.remove_node_from_partial_view(&peer_id);
                        println!("在网络中删除节点: {:?} {:?}", peer_id, multiaddr);
                    }
                }
            }
        }
    }
}
impl NetworkBehaviourEventProcess<PingEvent> for MyBehaviour {
    fn inject_event(&mut self, _event: PingEvent) {}
}
impl NetworkBehaviourEventProcess<FloodsubEvent> for MyBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        match event {
            FloodsubEvent::Message(message) => {
                let message = String::from_utf8_lossy(&message.data).to_string();
                println!("收到消息: {}", message);
                self.window
                    .emit_to("main", "receive", Message { data: message }).unwrap();
            }
            FloodsubEvent::Subscribed { peer_id, topic } => {
                println!("{:?} 订阅了 {}", peer_id, topic.id());
            }
            FloodsubEvent::Unsubscribed { peer_id, topic } => {
                println!("{:?} 取消订阅了 {}", peer_id, topic.id());
            }
        }
    }
}

pub struct Chat {
    swarm: Swarm<MyBehaviour>,
    topic: Topic,
    window: Window,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Message {
    data: String,
}

impl Chat {
    pub async fn new(
        id_keys: Keypair,
        peer_id: PeerId,
        window: Window,
    ) -> Result<Self, Box<dyn Error>> {
        let noise_key = noise::Keypair::<noise::X25519Spec>::new().into_authentic(&id_keys)?;
        let transport = TokioTcpTransport::new(GenTcpConfig::default().nodelay(true))
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::NoiseConfig::xx(noise_key).into_authenticated())
            .multiplex(yamux::YamuxConfig::default())
            .boxed();
        let topic = Topic::new("chat");
        let mut swarm = {
            let mut behaviour = MyBehaviour::new(peer_id, window.clone()).await?;
            behaviour.floodsub.subscribe(topic.clone());
            SwarmBuilder::new(transport, behaviour, peer_id)
                .executor(Box::new(|fut| {
                    tokio::spawn(fut);
                }))
                .build()
        };

        swarm.listen_on("/ip4/127.0.0.1/tcp/0".parse()?)?;

        Ok(Self {
            swarm,
            topic,
            window: window,
        })
    }

    pub async fn init(mut self) {
        loop {
            let (tx, rx) = oneshot::channel();
            let id = self.window.once_global("message", |event| {
                let message: Message = serde_json::from_str(event.payload().unwrap()).unwrap();
                tx.send(message.data).unwrap();
            });
            tokio::select! {
                message = rx => {
                    self.swarm.behaviour_mut().floodsub.publish(self.topic.clone(), message.unwrap().as_bytes())
                }
                event = self.swarm.select_next_some() => {
                    self.window.unlisten(id);
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            println!("本地监听地址: {address}");
                        }
                        SwarmEvent::Behaviour(event) => {
                            println!("event: {event:?}");
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
