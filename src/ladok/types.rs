use chrono::{NaiveDate, NaiveDateTime};
use serde::{de::Visitor, Deserialize, Deserializer, Serialize};
use std::convert::TryInto;
use std::fmt;
use std::num::NonZeroU32;

#[derive(Clone, Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Betygsgrad {
    GiltigSomSlutbetyg: bool,
    pub ID: BetygsgradID,
    Kod: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialOrd, Ord, PartialEq, Eq)]
pub struct BetygsgradID(NonZeroU32);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Betygskala {
    Betygsgrad: Vec<Betygsgrad>,
    ID: BetygsskalaID,
    pub Kod: String,
}

#[derive(Clone, Copy, Debug, Serialize, PartialOrd, Ord, PartialEq, Eq)]
pub struct BetygsskalaID(NonZeroU32);

impl fmt::Display for BetygsskalaID {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.0.fmt(f)
    }
}

impl<'de> Deserialize<'de> for BetygsskalaID {
    /// A custom deserializer, since the value sometimes appear as a quoted string i Ladok json.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
        D::Error: serde::de::Error,
    {
        struct MyVisitor;
        impl<'de> Visitor<'de> for MyVisitor {
            type Value = BetygsskalaID;

            fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt.write_str("integer or string")
            }

            fn visit_u32<E>(self, val: u32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match NonZeroU32::new(val) {
                    Some(val) => Ok(BetygsskalaID(val)),
                    None => Err(E::custom("invalid integer value")),
                }
            }

            fn visit_u64<E>(self, val: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(val.try_into().map_err(|_| E::custom("overflow"))?)
            }

            fn visit_str<E>(self, val: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(
                    val.parse()
                        .map_err(|_| E::custom("failed to parse integer"))?,
                )
            }
        }

        deserializer.deserialize_any(MyVisitor)
    }
}

impl Betygskala {
    pub fn get(&self, kod: &str) -> Option<&Betygsgrad> {
        self.Betygsgrad.iter().find(|b| b.Kod == kod)
    }
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_StudieresultatForRapporteringSokVarden
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct StudieresultatForRapporteringSokVarden {
    // ' dap:Base ' super type was not found in this schema. Some elements and attributes may be missing.
    pub Filtrering: Vec<String>, // rr:StudieresultatTillstandVidRapporteringEnum [0..*]
    // <rr:GruppUID> xs:string </rr:GruppUID> [0..1] (not used)
    pub KurstillfallenUID: Vec<String>,
    pub Limit: u32,
    /// very important to have order by otherwise you get really
    /// strange results with missing data and duplicate students
    pub OrderBy: Vec<String>, // rr:StudieresultatOrderByEnum [0..*]
    pub Page: u32,
    // <rr:StudenterUID> xs:string </rr:StudenterUID> [0..*] (not used)
    pub UtbildningsinstansUID: Option<String>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se.html#type_Student
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Student {
    Uid: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct LarosateID(NonZeroU32);

impl LarosateID {
    pub const KTH: LarosateID = LarosateID(unsafe { NonZeroU32::new_unchecked(29) });
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#type_Studieresultat
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Studieresultat {
    LarosateID: Option<LarosateID>,
    SenastSparad: Option<NaiveDateTime>,
    SenastAndradAv: Option<String>,
    pub Uid: Option<String>,
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
    pub fn get_arbetsunderlag(&self, kurstillf: &str) -> Option<&Resultat> {
        for rpu in &self.ResultatPaUtbildningar {
            if let Some(au) = rpu.Arbetsunderlag.as_ref() {
                if au.UtbildningsinstansUID.as_ref().map(AsRef::as_ref) == Some(kurstillf) {
                    return Some(au);
                }
            }
        }
        None
    }
    pub fn get_betygsskala(&self) -> Option<BetygsskalaID> {
        self.Rapporteringskontext
            .as_ref()
            .and_then(|rk| rk.BetygsskalaID)
    }
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_SokresultatStudieresultatResultat
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct SokresultatStudieresultatResultat {
    pub Resultat: Vec<Studieresultat>,
    pub TotaltAntalPoster: usize,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_Rapporteringskontext
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Rapporteringskontext {
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
pub struct ResultatPaUtbildning {
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
    pub fn find_student(&self, uid: &str) -> Option<&Studieresultat> {
        self.Resultat
            .iter()
            .find(|r| r.Student.as_ref().map(|s| s.Uid == uid).unwrap_or(false))
    }
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#type_SkapaResultat
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct SkapaResultat {
    pub Uid: Option<String>,
    pub Betygsgrad: Option<BetygsgradID>,
    pub BetygsskalaID: BetygsskalaID,
    pub Examinationsdatum: Option<NaiveDate>,
    /*
    <rr:ExamineradOmfattning> xs:decimal </rr:ExamineradOmfattning> [0..1]
    <rr:HanvisningTillBeslutshandling> ... </rr:HanvisningTillBeslutshandling> [0..1]
    <rr:Noteringar> rr:Notering </rr:Noteringar> [0..*]
    <rr:Projekttitel> ... </rr:Projekttitel> [0..1]
    <rr:AktivitetstillfalleUID> xs:string </rr:AktivitetstillfalleUID> [0..1]
     */
    pub StudieresultatUID: Option<String>,
    pub UtbildningsinstansUID: Option<String>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_SkapaFlera
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct SkapaFlera {
    pub LarosateID: LarosateID,
    pub Resultat: Vec<SkapaResultat>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_ResultatLista
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct UppdateraFlera {
    pub LarosateID: LarosateID,
    pub Resultat: Vec<UppdateraResultat>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#type_UppdateraResultat
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct UppdateraResultat {
    // <!-- ' base:BaseEntitet ' super type was not found in this schema. Some elements and attributes may be missing. -->
    pub Uid: Option<String>,

    pub Betygsgrad: Option<BetygsgradID>,
    pub BetygsskalaID: BetygsskalaID,
    pub Examinationsdatum: Option<NaiveDate>,
    //<rr:ExamineradOmfattning> xs:decimal </rr:ExamineradOmfattning> [0..1]
    //<rr:HanvisningTillBeslutshandling> ... </rr:HanvisningTillBeslutshandling> [0..1]
    //<rr:Noteringar> rr:Notering </rr:Noteringar> [0..*]
    //<rr:Projekttitel> ... </rr:Projekttitel> [0..1]
    //<rr:AktivitetstillfalleUID> xs:string </rr:AktivitetstillfalleUID> [0..1]
    pub ResultatUID: Option<String>,
    pub SenasteResultatandring: Option<NaiveDateTime>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#type_ResultatLista
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct ResultatLista {
    pub Resultat: Vec<Resultat>,
}

/// https://www.test.ladok.se/restdoc/schemas/schemas.ladok.se-resultat.html#element_Resultat
#[derive(Debug, Deserialize, Serialize)]
#[allow(non_snake_case)]
pub struct Resultat {
    pub Uid: Option<String>,
    AktivitetstillfalleUID: Option<String>,
    // <at:Beslut> ... </at:Beslut> [0..1]
    pub Betygsgrad: Option<BetygsgradID>,
    // <rr:Betygsgradsobjekt> rr:Betygsgrad </rr:Betygsgradsobjekt> [0..1]
    BetygsskalaID: Option<BetygsskalaID>,
    pub Examinationsdatum: Option<NaiveDate>,
    //<rr:ExamineradOmfattning> xs:decimal </rr:ExamineradOmfattning> [0..1]
    //<rr:ForbereddForBorttag> xs:boolean </rr:ForbereddForBorttag> [0..1]
    //<rr:HanvisningTillBeslutshandling> ... </rr:HanvisningTillBeslutshandling> [0..1]
    //<rr:Klarmarkering> rr:Klarmarkera </rr:Klarmarkering> [0..1]
    //<rr:KurstillfalleUID> xs:string </rr:KurstillfalleUID> [0..1]
    //<rr:Noteringar> rr:Notering </rr:Noteringar> [0..*]
    //<rr:ProcessStatus> xs:int </rr:ProcessStatus> [0..1]
    //<rr:Projekttitel> ... </rr:Projekttitel> [0..1]
    pub SenasteResultatandring: Option<NaiveDateTime>,
    StudieresultatUID: Option<String>,
    UtbildningsinstansUID: Option<String>,
}
