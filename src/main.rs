use std::{
    env,
    fs::OpenOptions,
    io::{BufWriter, Write},
    thread,
    time::Instant,
};

use log::info;
use miette::{bail, ensure, IntoDiagnostic, Result};
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use ureq::Agent;
use url::Url;

const CONNECTIONS: usize = 10;
const SPLIT_SIZE: usize = 20_000_000; // 20M

fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .into_diagnostic()?;
    ensure!(env::args().len() == 2, "Invalid number of arguments.");
    // We already make sure there's only 1 arg
    let url = Url::parse(&env::args().nth(1).unwrap()).into_diagnostic()?;
    let agent = Agent::new();

    let res = agent.head(url.as_str()).send_bytes(&[]).into_diagnostic()?;

    ensure!(
        res.has("Accept-Ranges"),
        "Server does not accept Range header! Cannot proceed."
    );

    let filesize = match res.has("Content-Length") {
        true => res
            .header("Content-Length")
            .unwrap()
            .parse::<usize>()
            .into_diagnostic()?,
        false => bail!("No file size found! Cannot download!"),
    };
    let filename = res.get_url().split('/').last().unwrap();

    let num_parts = filesize / SPLIT_SIZE;

    // We write file by appending
    let file = OpenOptions::new()
        .append(true)
        .create_new(true)
        .open(filename)
        .into_diagnostic()?;
    let mut writer = BufWriter::new(file);

    for part in 0..=num_parts {
        let segment_size = match part < num_parts {
            true => SPLIT_SIZE / CONNECTIONS,
            false => (filesize - part * SPLIT_SIZE) / CONNECTIONS,
        };

        // This is to measure speed
        let start = Instant::now();

        (0..CONNECTIONS)
            .map(|i| {
                let conn = agent.clone();
                let url = url.clone();
                thread::spawn(move || {
                    let mut body = Vec::with_capacity(segment_size);
                    let byte_range = match (part < num_parts, i < CONNECTIONS - 1) {
                        (true, true) | (true, false) | (false, true) => format!(
                            "bytes={}-{}",
                            i * segment_size + part * SPLIT_SIZE,
                            (i + 1) * segment_size + part * SPLIT_SIZE - 1
                        ),
                        (false, false) => {
                            format!("bytes={}-", i * segment_size + part * SPLIT_SIZE)
                        }
                    };
                    conn.get(url.as_str())
                        .set("Range", &byte_range)
                        .send_bytes(&[])
                        .unwrap()
                        .into_reader()
                        .read_to_end(&mut body)
                        .unwrap();
                    let duration = start.elapsed().as_secs_f64();
                    info!(
                        "Segment {i} downloaded in {duration}s, speed {0:.2} MB/s",
                        segment_size as f64 / (duration * 1_000_000.0)
                    );
                    body
                })
            })
            .for_each(|t| {
                writer
                    .write_all(&t.join().expect("Cannot get file data!"))
                    .into_diagnostic()
                    .expect("Cannot write to buffer!");
                writer
                    .flush()
                    .into_diagnostic()
                    .expect("Cannot flush writer!");
            })
    }
    info!("Finished downloading file {filename}");

    Ok(())
}
