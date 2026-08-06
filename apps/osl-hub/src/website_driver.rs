//! Real website driver contract.
//!
//! The implementation that talks to a browser lives behind this interface. The
//! backend job list is fixed here so higher-level website work cannot smuggle in
//! generic browser automation verbs.

use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebsiteDriverJob {
    FindPage,
    ReadPage,
    PlaceText,
    PressNamedControl,
}

impl WebsiteDriverJob {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::FindPage => "find_page",
            Self::ReadPage => "read_page",
            Self::PlaceText => "place_text",
            Self::PressNamedControl => "press_named_control",
        }
    }
}

pub const WEBSITE_DRIVER_JOBS: [WebsiteDriverJob; 4] = [
    WebsiteDriverJob::FindPage,
    WebsiteDriverJob::ReadPage,
    WebsiteDriverJob::PlaceText,
    WebsiteDriverJob::PressNamedControl,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePageRequest {
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePage {
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteTextPlacement {
    pub page: WebsitePage,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsiteNamedControl {
    pub page: WebsitePage,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebsitePageText {
    pub page: WebsitePage,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebsiteDriverError {
    PageNotFound,
    ReadFailed,
    TextPlacementFailed,
    NamedControlNotFound,
}

impl fmt::Display for WebsiteDriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PageNotFound => "website page was not found",
            Self::ReadFailed => "website page could not be read",
            Self::TextPlacementFailed => "website text could not be placed",
            Self::NamedControlNotFound => "website named control was not found",
        })
    }
}

impl std::error::Error for WebsiteDriverError {}

pub trait WebsiteDriver {
    const JOBS: &'static [WebsiteDriverJob] = &WEBSITE_DRIVER_JOBS;

    fn find_page(&mut self, request: WebsitePageRequest)
        -> Result<WebsitePage, WebsiteDriverError>;
    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError>;
    fn place_text(&mut self, placement: WebsiteTextPlacement) -> Result<(), WebsiteDriverError>;
    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NamesOnlyDriver;

    impl WebsiteDriver for NamesOnlyDriver {
        fn find_page(
            &mut self,
            request: WebsitePageRequest,
        ) -> Result<WebsitePage, WebsiteDriverError> {
            Ok(WebsitePage { url: request.url })
        }

        fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
            Ok(WebsitePageText {
                page: page.clone(),
                text: "visible page text".to_owned(),
            })
        }

        fn place_text(
            &mut self,
            _placement: WebsiteTextPlacement,
        ) -> Result<(), WebsiteDriverError> {
            Ok(())
        }

        fn press_named_control(
            &mut self,
            _control: WebsiteNamedControl,
        ) -> Result<(), WebsiteDriverError> {
            Ok(())
        }
    }

    #[test]
    fn task_1200_driver_interface_lists_exactly_four_real_website_jobs() {
        let jobs = <NamesOnlyDriver as WebsiteDriver>::JOBS;
        let names = jobs.iter().map(|job| job.wire_name()).collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "find_page",
                "read_page",
                "place_text",
                "press_named_control"
            ]
        );
        assert_eq!(jobs.len(), 4);

        println!("TASK1200 website_driver_job_count={}", jobs.len());
        for name in names {
            println!("TASK1200 website_driver_job={name}");
        }
    }
}
