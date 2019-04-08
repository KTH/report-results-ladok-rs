use failure::{format_err, Error};
use reqwest::{Client, Identity, RequestBuilder};
use serde::de::DeserializeOwned;
use serde_derive::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, BufRead, Read};
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
    ID: usize,
    Kod: String,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Betygskala {
    Betygsgrad: Vec<Betygsgrad>,
    ID: String,
    Kod: String,
}

impl Betygskala {
    fn get(&self, kod: &str) -> Option<&Betygsgrad> {
        self.Betygsgrad.iter().find(|b| b.Kod == kod)
    }
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
struct Studieresultat {
    LarosateID: Option<usize>,
    SenastSparad: Option<String>, // xs:dateTime
    SenastAndradAv: Option<String>,
    Uid: Option<String>,
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
    Resultat: Vec<Studieresultat>,
    TotaltAntalPoster: usize,
}

impl SokresultatStudieresultatResultat {
    fn find_student(&self, uid: &str) -> Option<&Studieresultat> {
        self.Resultat
            .iter()
            .find(|r| r.Student.as_ref().map(|s| s.Uid == uid).unwrap_or(false))
    }
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#type_SkapaResultat
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct SkapaResultat {
    Uid: Option<String>,
    Betygsgrad: Option<usize>,         // kort numeriskt id
    BetygsskalaID: usize,              // kort numeriskt id
    Examinationsdatum: Option<String>, // TODO: Use a date type
    /*
    <rr:ExamineradOmfattning> xs:decimal </rr:ExamineradOmfattning> [0..1]
    <rr:HanvisningTillBeslutshandling> ... </rr:HanvisningTillBeslutshandling> [0..1]
    <rr:Noteringar> rr:Notering </rr:Noteringar> [0..*]
    <rr:Projekttitel> ... </rr:Projekttitel> [0..1]
    <rr:AktivitetstillfalleUID> xs:string </rr:AktivitetstillfalleUID> [0..1]
     */
    StudieresultatUID: Option<String>,
    UtbildningsinstansUID: Option<String>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_SkapaFlera
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct SkapaFlera {
    LarosateID: usize, // KTH is 29
    Resultat: Vec<SkapaResultat>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_ResultatLista
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct UppdateraFlera {
    LarosateID: usize, // KTH is 29
    Resultat: Vec<UppdateraResultat>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#type_UppdateraResultat
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct UppdateraResultat {
    // <!-- ' base:BaseEntitet ' super type was not found in this schema. Some elements and attributes may be missing. -->
    Uid: Option<String>,

    Betygsgrad: Option<usize>,
    BetygsskalaID: usize,
    Examinationsdatum: Option<String>, // xs:date
    //<rr:ExamineradOmfattning> xs:decimal </rr:ExamineradOmfattning> [0..1]
    //<rr:HanvisningTillBeslutshandling> ... </rr:HanvisningTillBeslutshandling> [0..1]
    //<rr:Noteringar> rr:Notering </rr:Noteringar> [0..*]
    //<rr:Projekttitel> ... </rr:Projekttitel> [0..1]
    //<rr:AktivitetstillfalleUID> xs:string </rr:AktivitetstillfalleUID> [0..1]
    ResultatUID: Option<String>,
    //<rr:SenasteResultatandring> xs:dateTime </rr:SenasteResultatandring> [0..1]
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#type_ResultatLista
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct ResultatLista {
    Resultat: Vec<Resultat>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_Resultat
#[derive(Debug, Deserialize, Serialize)]
#[allow(non_snake_case)]
struct Resultat {
    Uid: Option<String>,
    AktivitetstillfalleUID: Option<String>,
    // <at:Beslut> ... </at:Beslut> [0..1]
    Betygsgrad: Option<usize>,
    // <rr:Betygsgradsobjekt> rr:Betygsgrad </rr:Betygsgradsobjekt> [0..1]
    BetygsskalaID: Option<usize>,
    Examinationsdatum: Option<String>,
    //<rr:ExamineradOmfattning> xs:decimal </rr:ExamineradOmfattning> [0..1]
    //<rr:ForbereddForBorttag> xs:boolean </rr:ForbereddForBorttag> [0..1]
    //<rr:HanvisningTillBeslutshandling> ... </rr:HanvisningTillBeslutshandling> [0..1]
    //<rr:Klarmarkering> rr:Klarmarkera </rr:Klarmarkering> [0..1]
    //<rr:KurstillfalleUID> xs:string </rr:KurstillfalleUID> [0..1]
    //<rr:Noteringar> rr:Notering </rr:Noteringar> [0..*]
    //<rr:ProcessStatus> xs:int </rr:ProcessStatus> [0..1]
    //<rr:Projekttitel> ... </rr:Projekttitel> [0..1]
    //<rr:SenasteResultatandring> xs:dateTime </rr:SenasteResultatandring> [0..1]
    StudieresultatUID: Option<String>,
    UtbildningsinstansUID: Option<String>,
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
        do_json_or_err(self.client.get(&format!(
            "https://{}/resultat/grunddata/betygsskala/{}",
            self.server, id
        )))
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
        do_json_or_err(self.client.put(&url).json(&data))
    }

    fn skapa_studieresultat(&self, data: &SkapaFlera) -> Result<ResultatLista, Error> {
        let url = format!("https://{}/resultat/studieresultat/skapa", self.server);
        do_json_or_err(self.client.post(&url).json(data))
    }

    fn uppdatera_studieresultat(&self, data: &UppdateraFlera) -> Result<ResultatLista, Error> {
        let url = format!("https://{}/resultat/studieresultat/uppdatera", self.server);
        do_json_or_err(self.client.put(&url).json(data))
    }
}

fn do_json_or_err<T>(request: RequestBuilder) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    let mut response = request.header("accept", "application/json").send()?;
    if let Err(e) = response.error_for_status_ref() {
        Err(format_err!(
            "Got {:?} on {:?}:\n{}\n",
            e.status(),
            e.url(),
            response
                .text()
                .as_ref()
                .map(|s| s.as_ref())
                .unwrap_or("(no data)"),
        ))
    } else {
        Ok(response.json()?)
    }
}

/// Get a studnet id and a grade from stdin
///
/// This emulates the part that gets things from canvas, it could just
/// return some hardcoded values, but I don't want to write a student
/// id in the source code.
fn read_result_to_report() -> Vec<(String, String)> {
    io::stdin()
        .lock()
        .lines()
        .map(|line| {
            let line = line.unwrap();
            let mut words = line.split_whitespace();
            (
                words.next().unwrap_or("").into(),
                words.next().unwrap_or("").into(),
            )
        })
        .collect()
}

fn main() -> Result<(), Error> {
    let ladok = Ladok::new("api.test.ladok.se", "../cert/rr.p12".as_ref())?;

    // TODO Use the correct betygskala for each reported result.
    let betygskala = ladok.get_betygskala(131657)?;

    // Kursen SE1010
    let _kursinstans = "7e0c378c-73d8-11e8-afa7-8e408e694e54";
    // SE1010 HT18 50110
    let kurstillf = "c601ee70-73da-11e8-b4e0-063f9afb40e3";
    // Diagnostisk uppgift på ovanstående instans
    let momentid_1 = "7ddee586-73d8-11e8-b4e0-063f9afb40e3";
    // TEN1 på ovanstående instans
    let _momentid_2 = "7dca24f2-73d8-11e8-b4e0-063f9afb40e3";

    let resultat = ladok.sok_studieresultat(kurstillf.into(), momentid_1.into())?;
    if resultat.Resultat.len() < resultat.TotaltAntalPoster {
        println!(
            "Warning: Paging in use, got {} of {} results",
            resultat.Resultat.len(),
            resultat.TotaltAntalPoster
        );
    }

    for (student, grade) in read_result_to_report() {
        let grade = betygskala
            .get(&grade)
            .ok_or_else(|| format_err!("Grade {:?} not in {}", grade, betygskala.Kod))?;
        let one = dbg!(resultat.find_student(&student))
            .ok_or_else(|| format_err!("Failed to find result for student {}", student))?;

        let mut data = SkapaFlera {
            LarosateID: 29, // KTH is 29
            Resultat: vec![],
        };
        data.Resultat.push(SkapaResultat {
            Uid: one.Uid.clone(),
            Betygsgrad: Some(grade.ID),
            BetygsskalaID: betygskala.ID.parse()?,
            Examinationsdatum: Some("2019-04-01".into()),
            StudieresultatUID: one.Uid.clone(),
            UtbildningsinstansUID: Some(momentid_1.into()),
        });
        dbg!(ladok.skapa_studieresultat(&dbg!(data))?);

        /*
        let data = UppdateraFlera {
            LarosateID: 29, // KTH is 29
            Resultat: vec![UppdateraResultat {
                Uid: one.Uid.clone(),
                Betygsgrad: Some(grade.ID),
                BetygsskalaID: betygskala.ID.parse()?,
                Examinationsdatum: Some("2019-04-01".into()),
                ResultatUID: one.Uid.clone(),
            }]
        };
        dbg!(ladok.uppdatera_studieresultat(&dbg!(data))?);
         */
    }
    Ok(())
}
