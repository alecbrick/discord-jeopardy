use rand::Rng;
use std::env;
use std::error;
use std::fs::File;

use std::io::BufReader;

use serde::{Deserialize, Serialize};
use serenity::client::Client as DiscordClient;
use serenity::{
    model::{channel::Message, gateway::Ready},
    prelude::*,
    utils::MessageBuilder,
};

mod message_queue;
use crate::message_queue::setup_amq_listener;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone)]
struct Question {
    category: String,
    air_date: String,
    question: String,
    value: Option<String>,
    answer: String,
    round: String,
    show_number: String,
}

struct Handler {
    questions: Vec<Question>,
    receiver: Arc<Mutex<Receiver<Vec<u8>>>>,
}

struct CurrentQuestion;

impl TypeMapKey for CurrentQuestion {
    type Value = Option<Question>;
}

impl Handler {
    pub fn new() -> Result<Self, Box<dyn error::Error>> {
        let file = File::open("JEOPARDY_QUESTIONS1.json").map_err(|e| Box::new(e))?;
        let reader = BufReader::new(file);
        println!("Loading questions...");
        let questions: Vec<Question> = serde_json::from_reader(reader).map_err(|e| Box::new(e))?;
        println!("Done!");

        let (receiver, _publish_channel) = setup_amq_listener();

        Ok(Handler {
            questions,
            receiver: Arc::new(Mutex::new(receiver)),
        })
    }
}

impl EventHandler for Handler {
    fn message(&self, context: Context, msg: Message) {
        let words: Vec<&str> = msg.content.split(" ").collect();
        if words.len() == 0 || words[0] != "trebek" {
            return;
        }

        if words.len() >= 3 && words[1] == "jeopardy" && words[2] == "me" {
            let mut data = context.data.write();
            let current_question_opt = data.get_mut::<CurrentQuestion>().unwrap();
            let mut response = MessageBuilder::new();
            if let Some(current_question) = current_question_opt {
                response.push(format!(
                    "The correct answer was {}.\n",
                    current_question.answer
                ));
            }

            let mut rng = rand::thread_rng();
            let index = rng.gen_range(0, self.questions.len());
            let question = self.questions[index].clone();
            let value = question.value.clone().unwrap_or("$200".to_string());
            response.push(format!(
                "The category is {}, for {}: {}",
                question.category, value, question.question
            ));
            *current_question_opt = Some(question);
            msg.channel_id
                .say(&context.http, &response.build())
                .expect("Failed to send new question!");
            return;
        }

        if words.len() >= 3 && words[1] == "what" {
            let mut data = context.data.write();
            let current_question_opt = data.get_mut::<CurrentQuestion>().unwrap();
            if let Some(current_question) = current_question_opt {
                let given_answer = words[2..].join(" ");
                if current_question.answer.to_ascii_lowercase() == given_answer.to_ascii_lowercase()
                {
                    msg.channel_id
                        .say(&context.http, "That's it!")
                        .expect("Failed to respond to correct answer");
                    *current_question_opt = None;
                    return;
                } else {
                    msg.channel_id
                        .say(&context.http, "Sorry, that's incorrect.")
                        .expect("Failed to respond to incorrect answer");
                    return;
                }
            } else {
                msg.channel_id
                    .say(
                        &context.http,
                        "I haven't given you a question yet. Cool your jets",
                    )
                    .expect("Failure to request cooling of jets");
                return;
            }
        }
        let channel = match msg.channel_id.to_channel(&context) {
            Ok(channel) => channel,
            Err(why) => {
                println!("Error getting channel: {:?}", why);

                return;
            }
        };

        let response = MessageBuilder::new()
            .push("User ")
            .push_bold_safe(&msg.author.name)
            .push(" used the 'ping' command in the ")
            .mention(&channel)
            .push(" channel")
            .build();

        if let Err(why) = msg.channel_id.say(&context.http, &response) {
            println!("Error sending message: {:?}", why);
        }
    }

    fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

fn main() -> Result<(), Box<dyn error::Error>> {
    let handler = Handler::new()?;
    let mut client = DiscordClient::new(&env::var("DISCORD_TOKEN").expect("token"), handler)
        .expect("Error creating client");

    {
        let mut data = client.data.write();
        data.insert::<CurrentQuestion>(None);
    }

    if let Err(why) = client.start() {
        println!("An error occurred while running the client: {:?}", why);
    };

    Ok(())
}
