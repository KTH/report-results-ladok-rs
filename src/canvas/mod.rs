use chrono::{DateTime, FixedOffset};
use failure::Error;
use reqwest::Client;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct CourseRoom {
    pub integration_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CourseSection {
    pub name: Option<String>,
    pub integration_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Assignment {
    pub id: i32,
    pub integration_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Submission {
    pub assignment_id: Option<i32>,
    pub grade: Option<String>,
    pub user: Option<User>,
    pub graded_at: Option<DateTime<FixedOffset>>,
    pub grader_id: Option<i32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: i32,
    pub name: Option<String>,
    pub integration_id: Option<String>,
}

pub struct Canvas {
    base_url: String,
    auth_key: String,
    client: Client,
}

impl Canvas {
    pub fn new(hostname: &str, auth_key: &str) -> Result<Canvas, Error> {
        Ok(Canvas {
            base_url: format!("https://{}/api/v1", hostname),
            auth_key: auth_key.into(),
            client: Client::builder().build()?,
        })
    }

    /// The sections of a course room is the real connection to ladok course rounds.
    ///
    /// sis_id will look like e.g. LT1016VT191.  Each element of the
    /// resulting section data may contain a ladok courseround oid in
    /// the integration_id field.
    pub fn get_course_sections(&self, sis_id: &str) -> Result<Vec<CourseSection>, Error> {
        Ok(self
            .client
            .get(&format!(
                "{}/courses/sis_course_id:{}/sections",
                self.base_url, sis_id
            ))
            .bearer_auth(&self.auth_key)
            .send()?
            .error_for_status()?
            .json()?)
    }
    pub fn get_assignments(&self, sis_id: &str) -> Result<Vec<Assignment>, Error> {
        Ok(self
            .client
            .get(&format!(
                "{}/courses/sis_course_id:{}/assignments",
                self.base_url, sis_id
            ))
            .bearer_auth(&self.auth_key)
            .send()?
            .error_for_status()?
            .json()?)
    }

    pub fn get_submissions(&self, sis_id: &str) -> Result<Vec<Submission>, Error> {
        let mut result = vec![];
        let mut next_url = Some(format!(
            "{}/courses/sis_course_id:{}/students/submissions?student_ids[]=all&include[]=user&per_page=100",
            self.base_url, sis_id
        ));
        while let Some(url) = next_url {
            let mut resp = self
                .client
                .get(&url)
                .bearer_auth(&self.auth_key)
                .send()?
                .error_for_status()?;
            next_url = resp
                .headers()
                .get("link")
                .and_then(|h| h.to_str().ok())
                .and_then(get_next_url);
            result.append(&mut resp.json()?);
            dbg!(result.len());
        }
        Ok(result)
    }
}

fn get_next_url(links: &str) -> Option<String> {
    const END: &str = ">; rel=\"next\"";
    for link in links.split(',') {
        if link.starts_with('<') && link.ends_with(END) {
            return Some(link[1..link.len() - END.len()].to_string());
        }
    }
    None
}

#[test]
fn test_get_next_url() {
    assert_eq!(
        get_next_url("<https://kth.test.instructure.com/api/v1/courses/7798/students/submissions?student_ids%5B%5D=all&page=first&per_page=100>; rel=\"current\",<https://kth.test.instructure.com/api/v1/courses/7798/students/submissions?student_ids%5B%5D=all&page=bookmark:WzY1OTU4MzZd&per_page=100>; rel=\"next\",<https://kth.test.instructure.com/api/v1/courses/7798/students/submissions?student_ids%5B%5D=all&page=first&per_page=100>; rel=\"first\""),
        // TODO: Is %5B%5D ok here?  Or do we need do urldecode []?
        Some("https://kth.test.instructure.com/api/v1/courses/7798/students/submissions?student_ids%5B%5D=all&page=bookmark:WzY1OTU4MzZd&per_page=100".to_string()),
    )
}

#[test]
fn test_get_next_url_0() {
    assert_eq!(
        get_next_url("<https://kth.test.instructure.com/api/v1/courses/7798/students/submissions?student_ids%5B%5D=all&page=first&per_page=100>; rel=\"current\",<https://kth.test.instructure.com/api/v1/courses/7798/students/submissions?student_ids%5B%5D=all&page=first&per_page=100>; rel=\"first\""),
        None,
    )
}
