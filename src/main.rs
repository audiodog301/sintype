use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use std::thread;

use std::io::{stdin, stdout, Write};

trait Generator: Send {
    fn next_sample(&mut self, sample_rate: f32) -> f32;
    fn input_control(&mut self, inputs: Vec<f32>);
    fn get_id(&self) -> &String;
}

enum Instruction {
    NewGenerator(Box<dyn Generator>),
    Delete(String),
}

//simple sawtooth oscillator
#[derive(Clone)]
struct Saw {
    frequency: f32,
    count: i32,
    val: f32,
    id: String,
}

impl Saw {
    pub fn new(frequency: f32, count: i32, val: f32, id: String) -> Self {
        //TODO fix indentation
        Self {
            frequency: frequency,
            count: count,
            val: val,
            id: id,
        }
    }

    fn set_frequency(&mut self, freq: f32) {
        self.frequency = freq;
    }
}

impl Generator for Saw {
    fn next_sample(&mut self, sample_rate: f32) -> f32 {
        if self.count >= (sample_rate / self.frequency) as i32 {
            self.count = 0;
        } else {
            self.count += 1;
        }

        if self.count == 0 {
            self.val = 1.0;
        } else {
            self.val -= 1.0 / (sample_rate / self.frequency);
        }

        self.val - 0.5
    }

    fn input_control(&mut self, inputs: Vec<f32>) {

    }

    fn get_id(&self) -> &String {
        &self.id
    }
}

fn main() {
    //get a sender and receiver to send data to the audio thread and retrieve ddata from the audio thread
    let (command_sender, command_receiver): (
        crossbeam_channel::Sender<Instruction>,
        crossbeam_channel::Receiver<Instruction>,
    ) = crossbeam_channel::bounded(1024);

    //make a vector for threads running.
    let mut children = vec![];

    //make the audio thread!
    children.push(thread::spawn( move ||  { 
    //ABANDON HOPE
    #[cfg(all(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd"), feature = "jack"))]
    
    // Manually check for flags. Can be passed through cargo with -- e.g.
    // cargo run --release --example beep --features jack -- --jack
    let host = if std::env::args()
        .collect::<String>()
        .contains(&String::from("--jack"))
    {
        cpal::host_from_id(cpal::available_hosts()
            .into_iter()
            .find(|id| *id == cpal::HostId::Jack)
            .expect(
                "make sure --features jack is specified. only works on OSes where jack is available",
            )).expect("jack host unavailable")
    } else {
        cpal::default_host()
    };

    #[cfg(any(
        not(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd")),
        not(feature = "jack")
    ))]
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .expect("failed to find a default output device");
    let config = device.default_output_config().unwrap();

    //run with the sample format given by the device's default output config
    match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into(), command_receiver.clone()).unwrap(),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into(), command_receiver.clone()).unwrap(),
        cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), command_receiver.clone()).unwrap(),
    };}));

    //REFIND YOUR HOPE
    loop {
        //grab an input for frequency. TODO: make this actually parse stuff and send different Instructions lol.
        let mut input = String::new();

        print!("> ");
        stdout().flush().expect("Error: Failed to flush stdout");

        stdin()
            .read_line(&mut input)
            .expect("Error: failed to read user input.");

        let input_parts: Vec<&str> = input.split(" ").collect();

        if input_parts[0] == "new" {
            command_sender.send(Instruction::NewGenerator(Box::new(Saw::new(input_parts[1].parse::<f32>().expect("ERROR PROBABLY"), 0, 0.0, String::from(input_parts[2])))));
        } else if input_parts[0] == "del" {
            command_sender.send(Instruction::Delete(String::from(input_parts[1])));
        }
    }
}

//LOSE SOME OF THE HOPE YOU GOT BACK BUT NOT ALL (seriously it's not too bad)
fn run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    command_receiver: crossbeam_channel::Receiver<Instruction>,
) -> Result<(), anyhow::Error>
where
    T: cpal::Sample,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let mut generators: Vec<Box<dyn Generator>> = Vec::new();

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            //for every buffer of audio sort of
            for frame in data.chunks_mut(channels) {
                //grab a sample from the sawtooth oscillator
                let mut out: f32 = 0f32;

                let current_gen_count = generators.len();

                for gen in &mut generators {
                    out += gen.next_sample(sample_rate) / current_gen_count as f32 / 3f32;
                }
                
                let value: T = cpal::Sample::from::<f32>(&out);

                //parse instructions
                while let Ok(instruction) = command_receiver.try_recv() {
                    match instruction {
                        Instruction::NewGenerator(generator) => {
                            generators.push(generator);
                        },
                        Instruction::Delete(id) => {
                            generators.retain(|x| *(x.get_id()) != id);
                        }
                    }
                }

                //for every sample of this buffer (left and right samples simultaneously at the same time I think.) TODO: understand how the frame works / how to access individual channels
                for sample in frame.iter_mut() {
                    //make it the sawtooth sample we got earlier
                    *sample = value
                }
            }
        },
        err_fn,
    )?;
    stream.play()?;

    //loops forever so the thread doesn't die immediately
    loop {}

    Ok(())
}
