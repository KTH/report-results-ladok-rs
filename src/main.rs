use dotenv::dotenv;
use failure::{format_err, Error};
use log::{error, info, warn};
use reqwest::{Client, Identity};
use serde::{Deserialize, Serialize};
use std::env::var;
use std::net::SocketAddr;
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

fn main() -> Result<(), Error> {
    let _ = dotenv();
    env_logger::init();
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
                .and(ctx.clone())
                .map(about)
                .or(path("_monitor").and(get()).map(monitor))
                .or(path("export")
                    .and(post())
                    .and(ctx.clone())
                    .and(body::form())
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

    let addr = var("LISTEN");
    let addr = addr
        .as_ref()
        .map(AsRef::as_ref)
        .unwrap_or("127.0.0.1:3030")
        .parse::<SocketAddr>()?;
    warp::serve(routes).run(addr);
    Ok(())
}

struct ServerContext {
    canvas_host: String, // hostname
    canvas_client_id: String,
    canvas_client_secret: String,
    ladok_base_url: String,
    ladok_key_data: Vec<u8>,
    ladok_key_pass: String,
    proxy_base: String,
}

impl ServerContext {
    fn from_env() -> Result<ServerContext, Error> {
        Ok(ServerContext {
            canvas_host: var2("CANVAS_HOST")?,
            canvas_client_id: var2("CANVAS_CLIENT_ID")?,
            canvas_client_secret: var2("CANVAS_CLIENT_SECRET")?,
            ladok_base_url: var2("LADOK_API_BASEURL")?,
            ladok_key_data: base64::decode(&var2("LADOK_API_PFX_BASE64")?)?,
            ladok_key_pass: var2("LADOK_API_PFX_PASSPHRASE")?,
            proxy_base: var2("PROXY_BASE")?,
        })
    }
    fn auth_canvas_client(&self, code: &str) -> Result<Canvas, Error> {
        #[derive(Serialize)]
        struct OathRequest<'a> {
            grant_type: &'a str,
            client_id: &'a str,
            client_secret: &'a str,
            redirect_uri: &'a str,
            code: &'a str,
        };
        #[derive(Debug, Deserialize)]
        struct CanvasUser {
            id: u32,
            name: String,
            global_id: String,
            effective_locale: Option<String>,
        }
        #[derive(Deserialize)]
        struct OathResponse {
            access_token: String,
            user: CanvasUser,
            // ignoring token_type, refresh_token and expires_in for now.
        }
        let oauth = Client::builder()
            .build()?
            .post(&format!("https://{}/login/oauth2/token", self.canvas_host))
            .json(&OathRequest {
                grant_type: "authorization_code",
                client_id: &self.canvas_client_id,
                client_secret: &self.canvas_client_secret,
                redirect_uri: &self.main_url(),
                code,
            })
            .header("accept", "application/json")
            .send()?
            .error_for_status()?
            .json::<OathResponse>()?;
        info!("Got access token for {:?}", oauth.user);
        Canvas::new(&self.canvas_host, &oauth.access_token)
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
    fn main_url(&self) -> String {
        format!("{}/api/{}/export", self.proxy_base, env!("CARGO_PKG_NAME"))
    }
    fn ladok_client(&self) -> Result<Ladok, Error> {
        Ladok::new(
            &self.ladok_base_url,
            Identity::from_pkcs12_der(&self.ladok_key_data, &self.ladok_key_pass)?,
        )
    }
}

fn var2(name: &str) -> Result<String, Error> {
    var(name).map_err(|e| format_err!("{}: {}", name, e))
}

fn about(ctx: Arc<ServerContext>) -> impl Reply {
    format!(
        "{} {}\n\nCanvas base: https://{}/\nLadok base: {}/\n",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        ctx.canvas_host,
        ctx.ladok_base_url,
    )
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
    let sis_course_id = b.lis_course_offering_sourcedid;
    let canvas_course_id = b.custom_canvas_course_id;
    let next_url = format!(
        "{}2?{}",
        ctx.main_url(),
        serde_urlencoded::to_string(QueryArgs {
            canvasCourseId: Some(canvas_course_id),
            error: None,
            code: b.code,
            sisCourseId: sis_course_id,
        })
        .unwrap(),
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
    code: Option<String>,
}

fn export_step_2(_ctx: Arc<ServerContext>, query: QueryArgs) -> impl Reply {
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
                       \n<script>document.location='export3?{}'</script>\n",
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
struct QueryArgs {
    canvasCourseId: Option<String>,
    error: Option<String>,
    code: Option<String>,
    sisCourseId: String,
}

fn export_step_3(ctx: Arc<ServerContext>, query: QueryArgs) -> impl Reply {
    info!(
        "Should export for {:?} / {:?}",
        query.sisCourseId, query.canvasCourseId,
    );

    let canvas = match ctx.auth_canvas_client(query.code.as_ref().unwrap()) {
        Ok(client) => client,
        Err(e) => {
            warn!("The access token cannot be retrieved from Canvas: {}", e);
            return access_denied("You should launch this application from a Canvas course");
        }
    };

    let result = (|| {
        let mut ladok = ctx.ladok_client()?;

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
    let grade = match &submission.grade {
        Some(ref grade) => grade,
        None => {
            info!("No grade for student {} in {:?}", student, submission);
            return Ok(ChangeToLadok::NoChange);
        }
    };
    let grade = ladok.get_grade(betygskala, grade)?;

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
