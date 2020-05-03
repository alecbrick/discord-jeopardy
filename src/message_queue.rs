use lapin::{
    message::DeliveryResult, options::*, types::FieldTable, BasicProperties, Channel, CloseOnDrop,
    Connection, ConnectionProperties, ConsumerDelegate,
};
use std::env;
use std::sync::mpsc::Receiver;

use std::sync::mpsc;
use std::sync::mpsc::SyncSender;

const QUEUE_NAME: &str = "audio";
const CONSUMER_NAME: &str = "interpret";

#[derive(Clone, Debug)]
struct Subscriber {
    channel: Channel,
    sender: SyncSender<Vec<u8>>,
}

impl ConsumerDelegate for Subscriber {
    fn on_new_delivery(&self, delivery: DeliveryResult) {
        let channel = self.channel.clone();
        let sender = self.sender.clone();
        Box::pin(async move {
            if let Ok(Some(delivery)) = delivery {
                channel
                    .basic_ack(delivery.delivery_tag, BasicAckOptions::default())
                    .await
                    .expect("basic_ack");
                if let Ok(_) = sender.send(delivery.data) {
                    println!("Data sent");
                }
            }
        });
    }
}

fn connect_amq() -> CloseOnDrop<Connection> {
    let broker_host: &str = &env::var("BROKER_HOST").unwrap();
    let broker_port: &str = &env::var("BROKER_PORT").unwrap();
    Connection::connect(
        &format!("amqp://{}:{}/%2f", broker_host, broker_port),
        ConnectionProperties::default(),
    )
    .wait()
    .expect("connection error")
}

pub fn setup_amq_listener() -> (Receiver<Vec<u8>>, CloseOnDrop<Channel>) {
    let conn = connect_amq();
    let publish_channel: CloseOnDrop<Channel> =
        conn.create_channel().wait().expect("create_channel");
    let subcribe_channel: CloseOnDrop<Channel> =
        conn.create_channel().wait().expect("create_channel");

    let (sender, receiver) = mpsc::sync_channel(1);
    attach_consumer(QUEUE_NAME, CONSUMER_NAME, &subcribe_channel, sender);

    (receiver, publish_channel)
}

fn declare_queue(publish_channel: &Channel, queue_name: &str) {
    publish_channel
        .queue_declare(
            queue_name,
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .wait()
        .expect("queue_declare");
}

pub fn send_amq_message(publish_channel: &Channel, queue_name: &str, payload: &[u8]) {
    publish_channel
        .basic_publish(
            "",
            queue_name,
            BasicPublishOptions::default(),
            payload.to_vec(),
            BasicProperties::default(),
        )
        .wait()
        .expect("basic_publish");
    println!("Payload sent to {}", queue_name);
}

fn attach_consumer(
    queue_name: &str,
    consumer_name: &'static str,
    subcribe_channel: &Channel,
    sender: SyncSender<Vec<u8>>,
) {
    subcribe_channel
        .queue_declare(
            queue_name,
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .wait()
        .expect("queue_declare");
    subcribe_channel
        .basic_consume(
            queue_name,
            consumer_name,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .wait()
        .expect("basic_consume")
        .set_delegate(Subscriber {
            channel: subcribe_channel.clone(),
            sender,
        });
    println!("Consumer attached to {}", queue_name);
}
