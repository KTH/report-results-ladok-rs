use failure::Error;
use reqwest::{Client, Identity};
use serde_derive::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

fn read_cert(file: &Path) -> Result<Identity, Error> {
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

#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct StudieresultatForRapporteringSokVarden {
    KurstillfallenUID: Vec<String>,
    Page: u32,
    Filtrering: Vec<String>,
    UtbildningsinstansUID: Option<String>,
    /// very important to have order by otherwise you get really
    /// strange results with missing data and duplicate students
    OrderBy: Vec<String>,
    Limit: u32,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se.html#type_Student
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Student {
    Uid: String,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#type_Studieresultat
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct StudieResultat {
    Uid: String,
    AktuellKursinstans: Option<String>,
    AktuelltKurstillfalle: Option<String>,
    // Anonymiseringskod: Option<String>, (ignorerar vi)
    // Avbrott ignorerar vi tills vidare
    KursUID: Option<String>,
    // Rapporteringskontext ignorerar vi tills vidare
    // ResultatPaUtbildningar:  ignorerar vi tills vidare
    SenastRegistrerad: Option<String>,
    Student: Option<Student>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct SokresultatStudieresultatResultat {
    Resultat: Vec<StudieResultat>,
    TotaltAntalPoster: usize,
}

struct Ladok {
    server: String,
    client: Client,
}

impl Ladok {
    fn new(server: &str, certpath: &Path) -> Result<Ladok, Error> {
        let key = read_cert(certpath)?;
        Ok(Ladok {
            server: server.to_string(),
            client: Client::builder().identity(key).build()?,
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

    fn sok_studieresultat(
        &self,
        kurstillf: String,
        moment: String,
    ) -> Result<SokresultatStudieresultatResultat, Error> {
        let url = format!(
            "https://{}/resultat/studieresultat/rapportera/utbildningsinstans/{}/sok",
            self.server, moment,
        );
        let data = StudieresultatForRapporteringSokVarden {
            KurstillfallenUID: vec![kurstillf],
            Page: 1,
            Filtrering: vec!["OBEHANDLADE".into(), "UTKAST".into()],
            UtbildningsinstansUID: Some(moment),
            OrderBy: vec![
                "EFTERNAMN_ASC".into(),
                "FORNAMN_ASC".into(),
                "PERSONNUMMER_ASC".into(),
            ],
            Limit: 100,
        };
        let mut result = self
            .client
            .put(&url)
            .header("accept", "application/json")
            .json(&data)
            .send()?;
        Ok(result.json()?)
    }
}

fn main() -> Result<(), Error> {
    let ladok = Ladok::new("api.test.ladok.se", "../cert/rr.p12".as_ref())?;

    // dbg!(ladok.get_betygskala(131657)?);

    // Kursen SE1010
    let _kursinstans = "7e0c378c-73d8-11e8-afa7-8e408e694e54";
    // SE1010 HT18 50110
    let kurstillf = "c601ee70-73da-11e8-b4e0-063f9afb40e3";
    // Diagnostisk uppgift på ovanstående instans
    let momentid_1 = "7ddee586-73d8-11e8-b4e0-063f9afb40e3";
    // TEN1 på ovanstående instans
    let _momentid_2 = "7dca24f2-73d8-11e8-b4e0-063f9afb40e3";

    let resultat = dbg!(ladok.sok_studieresultat(kurstillf.into(), momentid_1.into())?);
    if resultat.Resultat.len() < resultat.TotaltAntalPoster {
        println!(
            "Warning: Paging in use, got {} of {} results",
            resultat.Resultat.len(),
            resultat.TotaltAntalPoster
        );
    }

    Ok(())
}
