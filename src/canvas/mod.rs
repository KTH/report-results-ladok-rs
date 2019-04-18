use chrono::{DateTime, FixedOffset};
use failure::Error;
use reqwest::Client;
use serde_derive::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct CourseRoom {
    pub integration_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Assignment {
    pub id: u32,
    pub integration_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Submission {
    pub assignment_id: Option<u32>,
    pub grade: Option<String>,
    pub user_id: Option<u32>,
    pub graded_at: Option<DateTime<FixedOffset>>,
    pub grader_id: Option<u32>,
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

    /// A canvas course is really a course room, generally for a round.
    ///
    /// sis_id will look like e.g. SF1624xxx
    pub fn get_course(&self, sis_id: &str) -> Result<CourseRoom, Error> {
        Ok(self
            .client
            .get(&format!(
                "{}/courses/sis_course_id:{}",
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
    pub fn get_user_uid(&self, user_id: u32) -> Result<String, Error> {
        #[derive(Deserialize)]
        struct Data {
            data: String,
        }
        Ok(self
            .client
            .get(&format!(
                "{}/users/{}/custom_data/ladok_uid?ns=se.kth",
                self.base_url, user_id
            ))
            .bearer_auth(&self.auth_key)
            .send()?
            .error_for_status()?
            .json::<Data>()?
            .data)
    }

    pub fn get_submissions(&self, sis_id: &str) -> Result<Vec<Submission>, Error> {
        // TODO: Filter on assignement id in request?
        Ok(self
            .client
            .get(&format!(
                "{}/courses/sis_course_id:{}/students/submissions?student_ids[]=all&per_page=100",
                self.base_url, sis_id
            ))
            .bearer_auth(&self.auth_key)
            .send()?
            .error_for_status()?
            .json()?)
    }
}
