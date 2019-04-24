use failure::{format_err, Error};
use log::{error, info, warn};
use reqwest::{Client, Identity};
use serde::{Deserialize, Serialize};
use std::env::var;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use warp::filters::BoxedFilter;
use warp::http::{header, Response, StatusCode};
use warp::reject::custom;
use warp::{body, get2 as get, path, post2 as post, query, Filter, Reply};

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
    let context = Arc::new(ServerContext::from_env()?);
    let ctx: BoxedFilter<(Arc<ServerContext>,)> = warp::any()
        .and_then(move || Ok::<_, Error>(context.clone()).map_err(custom))
        .boxed();
    let routes = warp::any()
        .and(path("api"))
        .and(path(env!("CARGO_PKG_NAME")))
        .and(
            path("_about")
                .and(get())
                .map(about)
                .or(path("_monitor").and(get()).map(monitor))
                .or(path("export")
                    .and(post())
                    .and(ctx.clone())
                    .and(body::json())
                    .map(export_step_1))
                .or(path("export2")
                    .and(get())
                    .and(ctx.clone())
                    .and(query())
                    .map(export_step_2))
                .or(path("export3")
                    .and(get())
                    .and(ctx.clone())
                    .and(query())
                    .map(export_step_3)),
        );
    warp::serve(routes).run(([127, 0, 0, 1], 3030));
    Ok(())
}

struct ServerContext {
    canvas_host: String, // hostname
    canvas_client_id: String,
    canvas_client_secret: String,
}

impl ServerContext {
    fn from_env() -> Result<ServerContext, Error> {
        Ok(ServerContext {
            canvas_host: var("CANVAS_HOST")?,
            canvas_client_id: var("CANVAS_CLIENT_ID")?,
            canvas_client_secret: var("CANVAS_CLIENT_SECRET")?,
        })
    }
    fn auth_canvas_client(&self, redirect_uri: &str, code: &str) -> Result<Canvas, Error> {
        #[derive(Serialize)]
        struct OathRequest<'a> {
            grant_type: &'a str,
            client_id: &'a str,
            client_secret: &'a str,
            redirect_uri: &'a str,
            code: &'a str,
        };
        #[derive(Deserialize)]
        struct OathResponse {
            acces_token: String,
        }
        let access_token = Client::builder()
            .build()?
            .post(&format!("https://{}/login/oauth2/token", self.canvas_host))
            .json(&OathRequest {
                grant_type: "authorization_code",
                client_id: &self.canvas_client_id,
                client_secret: &self.canvas_client_secret,
                redirect_uri,
                code,
            })
            .header("accept", "application/json")
            .send()?
            .error_for_status()?
            .json::<OathResponse>()?
            .acces_token;
        Canvas::new(&self.canvas_host, &access_token)
    }
    fn get_oath_url(&self, next_url: &str) -> String {
        format!(
            "https://{}/login/oauth2/auth?{}",
            self.canvas_host,
            serde_urlencoded::to_string(&[
                ("client_id", self.canvas_client_id.as_str()),
                ("response_type", "code"),
                ("redirect_uri", &next_url),
            ])
            .unwrap(),
        )
    }
}

fn about() -> impl Reply {
    format!("{} {}\n", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
}

fn monitor() -> impl Reply {
    format!(
        "APPLICATION_STATUS: {} {}-{}",
        "OK",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    )
}

fn export_step_1(ctx: Arc<ServerContext>, b: ExportPostData) -> impl Reply {
    // const correlationId = req.id;
    eprintln!("Export request posted: {:?}", b);
    // console.log(`The user ${b.lis_person_sourcedid}, ${b.custom_canvas_user_login_id}, is exporting the course ${b.context_label} with id ${b.custom_canvas_course_id}`)
    let sis_course_id = b.lis_course_offering_sourcedid;
    let canvas_course_id = b.custom_canvas_course_id;
    let full_url = var("PROXY_BASE").unwrap(); // _or_else(|| format!("{}://{}", req.protocol, req.get("host"))) + req.originalUrl;
    let next_url = format!(
        "{}2?sisCourseId={}&canvasCourseId={}", // correlationId={}
        full_url,
        sis_course_id,
        canvas_course_id, // FIXME: urlencode
    );
    info!(
        "Tell auth to redirect back to {} using canvas client id {}",
        next_url, ctx.canvas_client_id,
    );

    let basic_url = ctx.get_oath_url(&next_url);
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, basic_url.clone())
        .body(format!("Please refer to {}", basic_url).into_bytes())
}

#[derive(Debug, Deserialize)]
struct ExportPostData {
    lis_course_offering_sourcedid: String,
    custom_canvas_course_id: String,
}

fn export_step_2(_ctx: Arc<ServerContext>, query: Step2QueryArgs) -> impl Reply {
    if query.canvasCourseId.is_none() {
        warn!("/export2 accessed with missing parameters. Ignoring the request...");
        return bad_request("The URL you are accessing needs extra parameters, please check it. If you came here by a link, inform us about this error.");
    }

    if let Some(error) = query.error {
        if error == "access_denied" {
            warn!("/export2 accessed without giving permission. Ignoring the request...");
            return bad_request("Access denied. You need to authorize this app to use it");
        }
        error!("/export2 accessed with an unexpected 'error' parameter which value is: {:?}. Ignoring the request...", error);
        return bad_request("An error ocurred. Please try it later.");
    }

    if query.code.is_none() {
        warn!("/export2 accessed without authorization code. Ignoring the request...");
        return bad_request("Access denied. You need to authorize this app to use it");
    }

    Response::builder()
        .body(format!("<link rel='stylesheet' href='/api/lms-export-results/kth-style/css/kth-bootstrap.css'>\
                       \n<div>Collecting all the data...</div>\
                       \n<script>document.location='exportResults3?{}'</script>\n",
                      serde_urlencoded::to_string(query).unwrap()).into_bytes()).unwrap()
}

fn bad_request(message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(
            format!(
            "<link rel='stylesheet' href='/api/lms-export-results/kth-style/css/kth-bootstrap.css'>\
             \n<div aria-live='polite' role='alert' class='alert alert-danger'>{}</div>\n",
            message,
            )
            .into_bytes(),
        )
        .unwrap()
}

fn access_denied(message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .body(format!(
            "<link rel='stylesheet' href='/api/lms-export-results/kth-style/css/kth-bootstrap.css'>\
             \n<div aria-live='polite' role='alert' class='alert alert-danger'>\
             \n<h3>Access denied</h3>\
             \n<p>{}</p>\
             \n<ul><li>If you have refreshed the browser, close the window or tab and launch it again from Canvas</li></ul>\
             \n</div>\n",
            message
        )
              .into_bytes()).unwrap()
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(non_snake_case)]
struct Step2QueryArgs {
    canvasCourseId: Option<String>,
    error: Option<String>,
    code: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(non_snake_case)]
struct Step3QueryArgs {
    canvasCourseId: Option<String>,
    //error: Option<String>,
    code: String,
    sisCourseId: String,
}

fn export_step_3(ctx: Arc<ServerContext>, query: Step3QueryArgs) -> impl Reply {
    info!(
        "Should export for {:?} / {:?}",
        query.sisCourseId, query.canvasCourseId,
    );

    let canvas = match ctx.auth_canvas_client(
        "https://app.kth.se/fixme/FIXME", // req.protocol + "://" + req.get("host") + req.originalUrl,
        &query.code,
    ) {
        Ok(client) => client,
        Err(e) => {
            warn!("The access token cannot be retrieved from Canvas: {}", e);
            return access_denied("You should launch this application from a Canvas course");
        }
    };

    let result = (|| {
        let mut ladok = Ladok::new("api.test.ladok.se", read_cert("../cert/rr.p12")?)?;

        do_report(&canvas, &mut ladok, &query.sisCourseId)
    })()
    .unwrap();

    Response::builder()
        .body(
            format!(
                "<html>\
                 \n<body>\
                 \n  <h1>Anropet till Ladok gick bra</h1>\
                 \n  <pre>{:#?}</pre>\
                 \n</body>\
                 \n</html>\n",
                result,
            )
            .into_bytes(),
        )
        .unwrap()
    /*} else {
      res.send('Inga resultat skickade :(')
    }*/
}

fn do_report(canvas: &Canvas, ladok: &mut Ladok, sis_courseroom: &str) -> Result<(), Error> {
    let kurstillf = canvas
        .get_course(sis_courseroom)?
        .integration_id
        .ok_or_else(|| format_err!("Canvas room {} is lacking integration id", sis_courseroom))?;

    let submissions = canvas.get_submissions(sis_courseroom)?;

    for assignment in canvas
        .get_assignments(sis_courseroom)?
        .into_iter()
        .filter(|a| a.integration_id.is_some())
    {
        let moment_id = assignment.integration_id.as_ref().unwrap();
        eprintln!(
            "Should report on moment {} on course {}",
            moment_id, kurstillf
        );
        let resultat = ladok.sok_studieresultat(&kurstillf, &moment_id)?;

        let mut create_queue = vec![];
        let mut update_queue = vec![];

        for submission in submissions
            .iter()
            .filter(|s| s.assignment_id == Some(assignment.id))
        {
            match dbg!(canvas.get_user_uid(dbg!(&submission.user_id).unwrap())).and_then(
                |student| prepare_ladok_change(ladok, student, &resultat, moment_id, submission),
            ) {
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
    }
    eprintln!("Ok.  Done.");
    Ok(())
}

fn prepare_ladok_change(
    ladok: &mut Ladok,
    student: String,
    resultat: &SokresultatStudieresultatResultat,
    moment_id: &str,
    submission: &Submission,
) -> Result<ChangeToLadok, Error> {
    let one = resultat
        .find_student(&student)
        .ok_or_else(|| format_err!("Failed to find result for student {}", student))?;

    let betygskala = one
        .get_betygsskala()
        .ok_or_else(|| format_err!("Missing Betygskala for student {}", student))?;
    let grade = ladok.get_grade(betygskala, &submission.grade.as_ref().unwrap())?;

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
