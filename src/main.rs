use failure::Error;
use reqwest::{Client, Identity};
use serde_derive::Deserialize;
use std::fs::File;
use std::io::Read;

fn read_cert() -> Result<Identity, Error> {
    let file = "../cert/rr.p12";
    let mut f = File::open(file)?;
    let mut data = vec![];
    f.read_to_end(&mut data)?;
    let id = Identity::from_pkcs12_der(&data, "")?;
    Ok(id)
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Betygsgrad {
    GiltigSomSlutbetyg: bool,
    ID: u32,
    Kod: String,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Betygskala {
    Betygsgrad: Vec<Betygsgrad>,
    ID: String,
    Kod: String,
}

struct Ladok {
    server: String,
    client: Client,
}

impl Ladok {
    fn new(server: &str) -> Result<Ladok, Error> {
        Ok(Ladok {
            server: server.to_string(),
            client: Client::builder().identity(read_cert()?).build()?,
        })
    }

    fn get_betygskala(&self, id: u32) -> Result<Betygskala, Error> {
        Ok(self
            .client
            .get(&format!(
                "https://{}/resultat/grunddata/betygsskala/{}",
                self.server, id
            ))
            .header("accept", "application/json")
            .send()?
            .json()?)
    }
}

fn main() -> Result<(), Error> {
    let ladok = Ladok::new("api.test.ladok.se")?;

    dbg!(ladok.get_betygskala(131657)?);

    Ok(())
}
