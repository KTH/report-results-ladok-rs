use failure::{format_err, Error};
use reqwest::Identity;
use std::env::var;
use std::fs::File;
use std::io::Read;
use std::path::Path;

mod canvas;
mod ladok;
use canvas::{Canvas, Submission};
use ladok::types::{SkapaResultat, SokresultatStudieresultatResultat, UppdateraResultat};
use ladok::Ladok;

fn read_cert<P: AsRef<Path>>(file: P) -> Result<Identity, Error> {
    let mut f = File::open(file)?;
    let mut data = vec![];
    f.read_to_end(&mut data)?;
    let id = Identity::from_pkcs12_der(&data, "")?;
    Ok(id)
}

fn main() -> Result<(), Error> {
    let sis_courseroom = "LT1016VT191";

    let canvas = Canvas::new("kth.test.instructure.com", &var("CANVAS_API_KEY")?)?;
    let kurstillf = canvas
        .get_course(sis_courseroom)?
        .integration_id
        .ok_or_else(|| format_err!("Canvas room {} is lacking integration id", sis_courseroom))?;
    let assignment = canvas
        .get_assignments(sis_courseroom)?
        .into_iter()
        .find(|a| a.integration_id.is_some())
        .unwrap();
    let moment_id = assignment.integration_id.as_ref().unwrap();
    eprintln!(
        "Should report on moment {} on course {}",
        moment_id, kurstillf
    );
    let submissions = canvas
        .get_submissions(sis_courseroom)?
        .into_iter()
        .filter(|s| s.assignment_id == Some(assignment.id))
        .collect::<Vec<_>>();

    let mut ladok = Ladok::new("api.test.ladok.se", read_cert("../cert/rr.p12")?)?;

    let resultat = ladok.sok_studieresultat(kurstillf, &moment_id)?;

    let mut create_queue = vec![];
    let mut update_queue = vec![];

    for submission in submissions {
        match canvas
            .get_user_uid(dbg!(&submission).user_id.unwrap())
            .and_then(|student| prepare_ladok_change(&mut ladok, student, &resultat, moment_id, submission))
        {
            Ok(ChangeToLadok::Update(data)) => update_queue.push(data),
            Ok(ChangeToLadok::Create(data)) => create_queue.push(data),
            Ok(ChangeToLadok::NoChange) => (),
            Err(e) => eprintln!("Error {}", e),
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

fn prepare_ladok_change(
    ladok: &mut Ladok,
    student: String,
    resultat: &SokresultatStudieresultatResultat,
    moment_id: &str,
    submission: Submission,
) -> Result<ChangeToLadok, Error> {
    let one = resultat
        .find_student(&student)
        .ok_or_else(|| format_err!("Failed to find result for student {}", student))?;

    let betygskala = one
        .get_betygsskala()
        .ok_or_else(|| format_err!("Missing Betygskala for student {}", student))?;
    let grade = ladok.get_grade(betygskala, &submission.grade.unwrap())?;

    let exam_date = submission
        .graded_at
        .ok_or_else(|| format_err!("Submission missing graded_at for student {}", student))?
        .naive_local()
        .date();

    Ok(if let Some(underlag) = one.get_arbetsunderlag(moment_id) {
        if underlag.Betygsgrad != Some(grade.ID) || underlag.Examinationsdatum != Some(exam_date) {
            eprintln!(
                "Updating grade from {:?} to {:?} for {:?}",
                underlag.Betygsgrad, grade, student
            );
            ChangeToLadok::Update(UppdateraResultat {
                Uid: one.Uid.clone(),
                Betygsgrad: Some(grade.ID),
                BetygsskalaID: betygskala,
                Examinationsdatum: Some(exam_date),
                ResultatUID: underlag.Uid.clone(),
                SenasteResultatandring: underlag.SenasteResultatandring,
            })
        } else {
            eprintln!("Grade {:?} up to date for {:?}", grade, student);
            ChangeToLadok::NoChange
        }
    } else {
        ChangeToLadok::Create(SkapaResultat {
            Uid: one.Uid.clone(),
            Betygsgrad: Some(grade.ID),
            BetygsskalaID: betygskala,
            Examinationsdatum: Some(exam_date),
            StudieresultatUID: one.Uid.clone(),
            UtbildningsinstansUID: Some(moment_id.to_string()),
        })
    })
}

enum ChangeToLadok {
    Update(UppdateraResultat),
    Create(SkapaResultat),
    NoChange,
}
