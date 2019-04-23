use failure::{format_err, Error};
use reqwest::{Client, Identity, RequestBuilder};
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;

pub mod types;
use types::*;

pub struct Ladok {
    server: String,
    client: Client,
    betygskalor_cache: BTreeMap<BetygsskalaID, Betygskala>,
}

impl Ladok {
    pub fn new(server: &str, client_identity: Identity) -> Result<Ladok, Error> {
        Ok(Ladok {
            server: server.to_string(),
            client: Client::builder().identity(client_identity).build()?,
            betygskalor_cache: BTreeMap::new(),
        })
    }

    fn get_betygskala(&self, id: BetygsskalaID) -> Result<Betygskala, Error> {
        do_json_or_err(self.client.get(&format!(
            "https://{}/resultat/grunddata/betygsskala/{}",
            self.server, id
        )))
    }

    pub fn get_grade(
        &mut self,
        betygskala: BetygsskalaID,
        grade: &str,
    ) -> Result<Betygsgrad, Error> {
        let betygskala = if let Some(betygskala) = self.betygskalor_cache.get(&betygskala) {
            betygskala
        } else {
            let loaded = self.get_betygskala(betygskala)?;
            self.betygskalor_cache.insert(betygskala, loaded);
            &self.betygskalor_cache[&betygskala]
        };
        betygskala
            .get(&grade)
            .cloned()
            .ok_or_else(|| format_err!("Grade {:?} not in {}", grade, betygskala.Kod))
    }

    pub fn sok_studieresultat(
        &self,
        kurstillf: &str,
        moment: &str,
    ) -> Result<SokresultatStudieresultatResultat, Error> {
        let url = format!(
            "https://{}/resultat/studieresultat/rapportera/utbildningsinstans/{}/sok",
            self.server, moment,
        );
        let mut data = StudieresultatForRapporteringSokVarden {
            KurstillfallenUID: vec![kurstillf.to_string()],
            Page: 1,
            Filtrering: vec!["OBEHANDLADE".into(), "UTKAST".into()],
            UtbildningsinstansUID: Some(moment.to_string()),
            OrderBy: vec![
                "EFTERNAMN_ASC".into(),
                "FORNAMN_ASC".into(),
                "PERSONNUMMER_ASC".into(),
            ],
            Limit: 100,
        };
        let mut resultat: SokresultatStudieresultatResultat =
            do_json_or_err(self.client.put(&url).json(&data))?;

        while resultat.Resultat.len() < resultat.TotaltAntalPoster {
            data.Page += 1;
            let r2: SokresultatStudieresultatResultat =
                do_json_or_err(self.client.put(&url).json(&data))?;
            resultat.Resultat.extend(r2.Resultat.into_iter());
        }
        println!(
            "Got {} of {} results, after fething {} page(s) of up to {} students.",
            resultat.Resultat.len(),
            resultat.TotaltAntalPoster,
            data.Page,
            data.Limit,
        );
        Ok(resultat)
    }

    pub fn skapa_studieresultat(&self, data: Vec<SkapaResultat>) -> Result<Vec<Resultat>, Error> {
        let url = format!("https://{}/resultat/studieresultat/skapa", self.server);
        Ok(
            do_json_or_err::<ResultatLista>(self.client.post(&url).json(&SkapaFlera {
                LarosateID: LarosateID::KTH,
                Resultat: data,
            }))?
            .Resultat,
        )
    }

    pub fn uppdatera_studieresultat(
        &self,
        data: Vec<UppdateraResultat>,
    ) -> Result<Vec<Resultat>, Error> {
        let url = format!("https://{}/resultat/studieresultat/uppdatera", self.server);
        Ok(
            do_json_or_err::<ResultatLista>(self.client.put(&url).json(&UppdateraFlera {
                LarosateID: LarosateID::KTH,
                Resultat: data,
            }))?
            .Resultat,
        )
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
                .map(AsRef::as_ref)
                .unwrap_or("(no data)"),
        ))
    } else {
        Ok(response.json()?)
    }
}
