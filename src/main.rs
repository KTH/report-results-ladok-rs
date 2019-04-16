use chrono::{NaiveDate, NaiveDateTime};
use failure::{format_err, Error};
use reqwest::{Client, Identity, RequestBuilder};
use serde::de::DeserializeOwned;
use serde::de::{Deserialize, Deserializer};
use serde_derive::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, BufRead, Read};
use std::num::NonZeroU32;
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
    ID: BetygsgradID,
    Kod: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct BetygsgradID(NonZeroU32);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Betygskala {
    Betygsgrad: Vec<Betygsgrad>,
    ID: BetygsskalaID,
    Kod: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(transparent)]
struct BetygsskalaID(NonZeroU32);

impl<'de> Deserialize<'de> for BetygsskalaID {
    /// A custom deserializer, since the value sometimes appear as a quoted string i Ladok json.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
        D::Error: serde::de::Error,
    {
        use serde::de::Error;
        use serde_json::Value;
        use std::convert::TryInto;
        let v = Value::deserialize(deserializer)?;
        let n = v
            .as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            .ok_or_else(|| D::Error::custom("non-integer"))?
            .try_into()
            .map_err(|_| D::Error::custom("overflow"))?;
        Ok(BetygsskalaID(
            NonZeroU32::new(n).ok_or_else(|| D::Error::custom("unexpected zero"))?,
        ))
    }
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct LarosateID(NonZeroU32);

impl LarosateID {
    const KTH: LarosateID = LarosateID(unsafe { NonZeroU32::new_unchecked(29) });
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#type_Studieresultat
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Studieresultat {
    LarosateID: Option<LarosateID>,
    SenastSparad: Option<NaiveDateTime>,
    SenastAndradAv: Option<String>,
    Uid: Option<String>,
    AktuellKursinstans: Option<String>,
    AktuelltKurstillfalle: Option<String>,
    // Anonymiseringskod: Option<String>, (ignorerar vi)
    // Avbrott ignorerar vi tills vidare
    KursUID: Option<String>,
    Rapporteringskontext: Option<Rapporteringskontext>,
    ResultatPaUtbildningar: Vec<ResultatPaUtbildning>,
    SenastRegistrerad: Option<NaiveDateTime>,
    Student: Option<Student>,
}

impl Studieresultat {
    fn get_arbetsunderlag(&self, kurstillf: &str) -> Option<&Resultat> {
        for rpu in &self.ResultatPaUtbildningar {
            if let Some(au) = rpu.Arbetsunderlag.as_ref() {
                if au.UtbildningsinstansUID.as_ref().map(|s| s.as_ref()) == Some(kurstillf) {
                    return Some(au);
                }
            }
        }
        None
    }
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_SokresultatStudieresultatResultat
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct SokresultatStudieresultatResultat {
    Resultat: Vec<Studieresultat>,
    TotaltAntalPoster: usize,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_Rapporteringskontext
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Rapporteringskontext {
    Anonymiseringskod: Option<String>,
    BetygsskalaID: Option<BetygsskalaID>,
    KravPaHanvisningTillBeslutshandling: bool,
    KravPaProjekttitel: bool,
    UtbildningUID: String,
    UtbildningsinstansUID: String,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_ResultatPaUtbildning
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct ResultatPaUtbildning {
    // ' dap:Base ' super type was not found in this schema. Some elements and attributes may be missing.
    Arbetsunderlag: Option<Resultat>,
    HarTillgodoraknande: Option<bool>,
    HeltTillgodoraknad: Option<bool>,
    KanExkluderas: Option<bool>,
    SenastAttesteradeResultat: Option<Resultat>,
    // <rr:TotalTillgodoraknadOmfattning> xs:decimal </rr:TotalTillgodoraknadOmfattning> [0..1]
    UtbildningUID: Option<String>,
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
    Betygsgrad: Option<BetygsgradID>,
    BetygsskalaID: BetygsskalaID,
    Examinationsdatum: Option<NaiveDate>,
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
    LarosateID: LarosateID,
    Resultat: Vec<SkapaResultat>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_ResultatLista
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct UppdateraFlera {
    LarosateID: LarosateID,
    Resultat: Vec<UppdateraResultat>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#type_UppdateraResultat
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct UppdateraResultat {
    // <!-- ' base:BaseEntitet ' super type was not found in this schema. Some elements and attributes may be missing. -->
    Uid: Option<String>,

    Betygsgrad: Option<BetygsgradID>,
    BetygsskalaID: BetygsskalaID,
    Examinationsdatum: Option<NaiveDate>,
    //<rr:ExamineradOmfattning> xs:decimal </rr:ExamineradOmfattning> [0..1]
    //<rr:HanvisningTillBeslutshandling> ... </rr:HanvisningTillBeslutshandling> [0..1]
    //<rr:Noteringar> rr:Notering </rr:Noteringar> [0..*]
    //<rr:Projekttitel> ... </rr:Projekttitel> [0..1]
    //<rr:AktivitetstillfalleUID> xs:string </rr:AktivitetstillfalleUID> [0..1]
    ResultatUID: Option<String>,
    SenasteResultatandring: Option<NaiveDateTime>,
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
    Betygsgrad: Option<BetygsgradID>,
    // <rr:Betygsgradsobjekt> rr:Betygsgrad </rr:Betygsgradsobjekt> [0..1]
    BetygsskalaID: Option<BetygsskalaID>,
    Examinationsdatum: Option<NaiveDate>,
    //<rr:ExamineradOmfattning> xs:decimal </rr:ExamineradOmfattning> [0..1]
    //<rr:ForbereddForBorttag> xs:boolean </rr:ForbereddForBorttag> [0..1]
    //<rr:HanvisningTillBeslutshandling> ... </rr:HanvisningTillBeslutshandling> [0..1]
    //<rr:Klarmarkering> rr:Klarmarkera </rr:Klarmarkering> [0..1]
    //<rr:KurstillfalleUID> xs:string </rr:KurstillfalleUID> [0..1]
    //<rr:Noteringar> rr:Notering </rr:Noteringar> [0..*]
    //<rr:ProcessStatus> xs:int </rr:ProcessStatus> [0..1]
    //<rr:Projekttitel> ... </rr:Projekttitel> [0..1]
    SenasteResultatandring: Option<NaiveDateTime>,
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

    let mut create_queue = vec![];
    let mut update_queue = vec![];

    for (student, grade) in read_result_to_report() {
        let grade = betygskala
            .get(&grade)
            .ok_or_else(|| format_err!("Grade {:?} not in {}", grade, betygskala.Kod))?;
        let one = dbg!(resultat.find_student(&student))
            .ok_or_else(|| format_err!("Failed to find result for student {}", student))?;

        let exam_date = NaiveDate::from_ymd(2019, 4, 16);
        if let Some(underlag) = dbg!(one.get_arbetsunderlag(momentid_1)) {
            update_queue.push(UppdateraResultat {
                Uid: one.Uid.clone(),
                Betygsgrad: Some(grade.ID),
                BetygsskalaID: betygskala.ID,
                Examinationsdatum: Some(exam_date),
                ResultatUID: underlag.Uid.clone(),
                SenasteResultatandring: underlag.SenasteResultatandring.clone(),
            });
        } else {
            create_queue.push(SkapaResultat {
                Uid: one.Uid.clone(),
                Betygsgrad: Some(grade.ID),
                BetygsskalaID: betygskala.ID,
                Examinationsdatum: Some(exam_date),
                StudieresultatUID: one.Uid.clone(),
                UtbildningsinstansUID: Some(momentid_1.into()),
            });
        }
    }

    if !create_queue.is_empty() {
        let data = SkapaFlera {
            LarosateID: LarosateID::KTH,
            Resultat: create_queue,
        };
        dbg!(ladok.skapa_studieresultat(&dbg!(data))?);
    }
    if !update_queue.is_empty() {
        let data = UppdateraFlera {
            LarosateID: LarosateID::KTH,
            Resultat: update_queue,
        };
        dbg!(ladok.uppdatera_studieresultat(&dbg!(data))?);
    }
    Ok(())
}
