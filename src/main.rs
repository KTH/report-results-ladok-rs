use dotenv::dotenv;
use failure::{format_err, Error};
use log::{error, info, warn};
use reqwest::{Client, Identity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env::var;
use std::net::SocketAddr;
use std::sync::Arc;
use warp::filters::path::Tail;
use warp::filters::BoxedFilter;
use warp::http::{header, Response, StatusCode};
use warp::reject::custom;
use warp::{body, get2 as get, path, post2 as post, query, Filter, Rejection, Reply};

mod canvas;
mod ladok;
use canvas::{Canvas, Submission};
use ladok::types::{SkapaResultat, SokresultatStudieresultatResultat, UppdateraResultat};
use ladok::Ladok;
use templates::RenderRucte;

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
                .or(path("s").and(path::tail()).and_then(static_file))
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

/// Handler for static files.
/// Create a response from the file data with a correct content type
/// and a far expires header (or a 404 if the file does not exist).
fn static_file(name: Tail) -> Result<impl Reply, Rejection> {
    use templates::statics::StaticFile;
    if let Some(data) = StaticFile::get(name.as_str()) {
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, data.mime.as_ref())
            // TODO: .far_expires()
            .body(data.content))
    } else {
        println!("Static file {:?} not found", name);
        Err(warp::reject::not_found())
    }
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
    Response::builder()
        .html(|o| templates::about(o, &ctx.canvas_host, &ctx.ladok_base_url))
        .unwrap()
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
            return access_denied();
        }
        error!("/export2 accessed with an unexpected 'error' parameter which value is: {:?}. Ignoring the request...", error);
        return bad_request("An error ocurred. Please try it later.");
    }

    if query.code.is_none() {
        warn!("/export2 accessed without authorization code. Ignoring the request...");
        return bad_request("Access denied. You need to authorize this app to use it");
    }

    Response::builder()
        .html(|o| {
            templates::collecting(
                o,
                &query.sisCourseId,
                &format!("export3?{}", serde_urlencoded::to_string(&query).unwrap()),
            )
        })
        .unwrap()
}

fn bad_request(message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .html(|o| templates::error(o, StatusCode::BAD_REQUEST, message))
        .unwrap()
}

fn access_denied() -> Response<Vec<u8>> {
    let status = StatusCode::UNAUTHORIZED;
    let msg = "You should launch this application from a Canvas course";
    Response::builder()
        .status(status)
        .html(|o| templates::error(o, status, msg))
        .unwrap()
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
            return access_denied();
        }
    };

    let result = (|| {
        let mut ladok = ctx.ladok_client()?;

        do_report(&canvas, &mut ladok, &query.sisCourseId)
    })()
    .unwrap();

    Response::builder()
        .html(|o| templates::done(o, result))
        .unwrap()
}

fn do_report(
    canvas: &Canvas,
    ladok: &mut Ladok,
    sis_courseroom: &str,
) -> Result<ExportResults, Error> {
    let kurstillf = canvas
        .get_course(sis_courseroom)?
        .integration_id
        .ok_or_else(|| format_err!("Canvas room {} is lacking integration id", sis_courseroom))?;

    let submissions = canvas.get_submissions(sis_courseroom)?;
    let mut retval = ExportResults::new();

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
            if let Some(canvas_user) = submission.user_id {
                match canvas.get_user_uid(canvas_user).and_then(|student| {
                    prepare_ladok_change(ladok, &student, &resultat, moment_id, submission)
                }) {
                    Ok(ChangeToLadok::Update(data, grade)) => {
                        update_queue.push(data);
                        retval.add(canvas_user, &format!(" Updated ({}) ", grade));
                    }
                    Ok(ChangeToLadok::Create(data, grade)) => {
                        create_queue.push(data);
                        retval.add(canvas_user, &format!(" Created ({}) ", grade));
                    }
                    Ok(ChangeToLadok::NoChange(grade)) => {
                        retval.add(canvas_user, &format!(" No change ({}) ", grade));
                    }
                    Ok(ChangeToLadok::NoGrade) => {
                        retval.add(canvas_user, " No grade ");
                    }
                    Err(e) => {
                        eprintln!("Error {}", e);
                        retval.add(canvas_user, &format!(" Error ({})", e));
                    }
                }
            }
        }
        info!(
            "There are {} results to create and {} to update",
            create_queue.len(),
            update_queue.len(),
        );
        if !create_queue.is_empty() {
            retval.created = ladok
                .skapa_studieresultat(create_queue)
                .map(|result| result.len())
                .map_err(|e| e.to_string())
        }
        if !update_queue.is_empty() {
            retval.updated = ladok
                .uppdatera_studieresultat(update_queue)
                .map(|result| result.len())
                .map_err(|e| e.to_string());
        }
    }
    info!("Ok.  Done.");
    Ok(retval)
}

#[derive(Debug)]
pub struct ExportResults {
    students: BTreeMap<u32, String>,
    created: Result<usize, String>,
    updated: Result<usize, String>,
}

impl ExportResults {
    fn new() -> Self {
        ExportResults {
            students: BTreeMap::new(),
            created: Ok(0),
            updated: Ok(0),
        }
    }
    fn add(&mut self, student: u32, status: &str) {
        self.students.entry(student).or_default().push_str(status);
    }
}

fn prepare_ladok_change(
    ladok: &mut Ladok,
    student: &str,
    resultat: &SokresultatStudieresultatResultat,
    moment_id: &str,
    submission: &Submission,
) -> Result<ChangeToLadok, Error> {
    let grade = match &submission.grade {
        Some(ref grade) => grade.to_uppercase(),
        None => return Ok(ChangeToLadok::NoGrade),
    };

    let one = resultat
        .find_student(&student)
        .ok_or_else(|| format_err!("Student {} not in Ladok result-list", student))?;

    let betygskala = one
        .get_betygsskala()
        .ok_or_else(|| format_err!("Missing Betygskala for student {}", student))?;

    let grade = ladok.get_grade(betygskala, &grade)?;

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
            ChangeToLadok::Update(
                UppdateraResultat {
                    Uid: one.Uid.clone(),
                    Betygsgrad: Some(grade.ID),
                    BetygsskalaID: betygskala,
                    Examinationsdatum: Some(exam_date),
                    ResultatUID: underlag.Uid.clone(),
                    SenasteResultatandring: underlag.SenasteResultatandring,
                },
                grade.Kod.clone(),
            )
        } else {
            eprintln!("Grade {:?} up to date for {:?}", grade, student);
            ChangeToLadok::NoChange(grade.Kod.clone())
        }
    } else {
        ChangeToLadok::Create(
            SkapaResultat {
                Uid: one.Uid.clone(),
                Betygsgrad: Some(grade.ID),
                BetygsskalaID: betygskala,
                Examinationsdatum: Some(exam_date),
                StudieresultatUID: one.Uid.clone(),
                UtbildningsinstansUID: Some(moment_id.to_string()),
            },
            grade.Kod.clone(),
        )
    })
}

enum ChangeToLadok {
    Update(UppdateraResultat, String),
    Create(SkapaResultat, String),
    NoChange(String),
    NoGrade,
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));
