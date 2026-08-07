mod row_who_wrote_it {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum SharedRowWhoWroteIt {
        Yours,
        Theirs,
        NotPublishedByApp,
    }
}

mod website_driver {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum WebsiteDriverJob {
        FindPage,
        ReadPage,
        PlaceText,
        ReadEditableBox,
        PressNamedControl,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum WebsiteDriverKind {
        RealBrowser,
        FakeTestBrowser,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct WebsitePageRequest {
        pub url: String,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct WebsitePage {
        pub url: String,
        pub target_id: Option<String>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum WebsiteControlKind {
        EditableBox,
        Button,
        VisibleMessageArea,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct WebsiteNamedControlRequest {
        pub name: &'static str,
        pub kind: WebsiteControlKind,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct WebsiteNamedControl {
        pub page: WebsitePage,
        pub name: String,
        pub kind: WebsiteControlKind,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct WebsiteTextPlacement {
        pub page: WebsitePage,
        pub editable_box_name: String,
        pub text: String,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct WebsitePlacementProof {
        pub page: WebsitePage,
        pub editable_box_name: String,
        pub utf16_units: usize,
        pub placed_sha256: String,
        pub readback_text: String,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct WebsitePageControls {
        pub editable_boxes: Vec<String>,
        pub buttons: Vec<String>,
        pub visible_message_areas: Vec<String>,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct WebsitePageText {
        pub page: WebsitePage,
        pub title: String,
        pub text: String,
        pub controls: WebsitePageControls,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum WebsiteDriverError {
        PageUnavailable,
        NamedControlNotFound,
        MissingNamedControl(String),
        TextPlacementFailed,
    }

    pub trait WebsiteDriver {
        const JOBS: &'static [WebsiteDriverJob] = &[];

        fn kind(&self) -> WebsiteDriverKind {
            WebsiteDriverKind::RealBrowser
        }

        fn find_page(
            &mut self,
            request: WebsitePageRequest,
        ) -> Result<WebsitePage, WebsiteDriverError>;

        fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError>;

        fn place_text(
            &mut self,
            placement: WebsiteTextPlacement,
        ) -> Result<WebsitePlacementProof, WebsiteDriverError>;

        fn press_named_control(
            &mut self,
            control: WebsiteNamedControl,
        ) -> Result<(), WebsiteDriverError>;

        fn read_named_controls(
            &mut self,
            page: &WebsitePage,
            required: &[WebsiteNamedControlRequest],
        ) -> Result<Vec<WebsiteNamedControl>, WebsiteDriverError> {
            let snapshot = self.read_page(page)?;
            required
                .iter()
                .map(|request| {
                    let present = match request.kind {
                        WebsiteControlKind::EditableBox => snapshot
                            .controls
                            .editable_boxes
                            .iter()
                            .any(|name| name == request.name),
                        WebsiteControlKind::Button => snapshot
                            .controls
                            .buttons
                            .iter()
                            .any(|name| name == request.name),
                        WebsiteControlKind::VisibleMessageArea => snapshot
                            .controls
                            .visible_message_areas
                            .iter()
                            .any(|name| name == request.name),
                    };
                    if present {
                        Ok(WebsiteNamedControl {
                            page: page.clone(),
                            name: request.name.to_owned(),
                            kind: request.kind,
                        })
                    } else {
                        Err(WebsiteDriverError::MissingNamedControl(
                            request.name.to_owned(),
                        ))
                    }
                })
                .collect()
        }
    }
}

#[path = "src/service_connections.rs"]
mod service_connections;
