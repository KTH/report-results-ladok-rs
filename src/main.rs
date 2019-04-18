use chrono::NaiveDate;
use failure::{format_err, Error};
use reqwest::Identity;
use std::fs::File;
use std::io::{self, BufRead, Read};
use std::path::Path;

mod ladok;
use ladok::types::{SkapaResultat, UppdateraResultat};
use ladok::Ladok;

fn read_cert<P: AsRef<Path>>(file: P) -> Result<Identity, Error> {
    let mut f = File::open(file)?;
    let mut data = vec![];
    f.read_to_end(&mut data)?;
    let id = Identity::from_pkcs12_der(&data, "")?;
    Ok(id)
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
    let mut ladok = Ladok::new("api.test.ladok.se", read_cert("../cert/rr.p12")?)?;

    // Kursen SE1010
    let _kursinstans = "7e0c378c-73d8-11e8-afa7-8e408e694e54";
    // SE1010 HT18 50110
    let kurstillf = "c601ee70-73da-11e8-b4e0-063f9afb40e3";
    // Diagnostisk uppgift på ovanstående instans
    let momentid_1 = "7ddee586-73d8-11e8-b4e0-063f9afb40e3";
    // TEN1 på ovanstående instans
    let _momentid_2 = "7dca24f2-73d8-11e8-b4e0-063f9afb40e3";

    let resultat = ladok.sok_studieresultat(kurstillf.into(), momentid_1.into())?;

    let mut create_queue = vec![];
    let mut update_queue = vec![];

    for (student, grade) in read_result_to_report() {
        let one = resultat
            .find_student(&student)
            .ok_or_else(|| format_err!("Failed to find result for student {}", student))?;

        // TODO: Get exam_date from canvas.
        let exam_date = NaiveDate::from_ymd(2019, 4, 16);

        let betygskala = one
            .get_betygsskala()
            .ok_or_else(|| format_err!("Missing Betygskala for student {}", student))?;
        let grade = ladok.get_grade(betygskala, &grade)?;

        if let Some(underlag) = one.get_arbetsunderlag(momentid_1) {
            update_queue.push(UppdateraResultat {
                Uid: one.Uid.clone(),
                Betygsgrad: Some(grade.ID),
                BetygsskalaID: betygskala,
                Examinationsdatum: Some(exam_date),
                ResultatUID: underlag.Uid.clone(),
                SenasteResultatandring: underlag.SenasteResultatandring,
            });
        } else {
            create_queue.push(SkapaResultat {
                Uid: one.Uid.clone(),
                Betygsgrad: Some(grade.ID),
                BetygsskalaID: betygskala,
                Examinationsdatum: Some(exam_date),
                StudieresultatUID: one.Uid.clone(),
                UtbildningsinstansUID: Some(momentid_1.into()),
            });
        }
    }
    eprintln!(
        "There are {} results to create and {} to update",
        create_queue.len(),
        update_queue.len(),
    );
    if !create_queue.is_empty() {
        let result = ladok.skapa_studieresultat(create_queue)?;
        eprintln!("After create: {} results", result.len())
    }
    if !update_queue.is_empty() {
        let result = ladok.uppdatera_studieresultat(update_queue)?;
        eprintln!("After update: {} results", result.len())
    }
    eprintln!("Ok.  Done.");
    Ok(())
}
