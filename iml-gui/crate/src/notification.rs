use crate::components::ActivityHealth;
use crate::Msg;
use iml_wire_types::{Alert, AlertSeverity};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Notification as N, NotificationOptions as NO, NotificationPermission as NP,
    ServiceWorkerRegistration,
};

pub(crate) struct Notification {
    svc: Option<ServiceWorkerRegistration>,
}

impl Notification {
    pub(crate) fn new() -> Self {
        Notification {
            svc: None,
        }
    }

    pub(crate) fn set_worker(self: &mut Self, js: JsValue) {
        self.svc = Some(ServiceWorkerRegistration::from(js));
    }

    /// Update existing notification, but do nothing if no notification shown.
    pub(crate) fn update(self: &Self, ah: &ActivityHealth) {
        if let Some(svc) = &self.svc {}
    }

    /// Display new incoming alert if it is severe enough and active.
    /// It should be called before updating activity health.
    pub(crate) fn display(self: &Self, a: &Alert, ah: &ActivityHealth) {
        if a.active.unwrap_or(false) && a.severity > AlertSeverity::INFO {
            let mut opts = NO::new();
            opts.tag(&"iml-alert")
                .icon(&"/favicon.ico")
                .require_interaction(true);

            if ah.count > 0 {
                opts.body(format!("+{} more", ah.count).as_str());
            }

            if let Some(svc) = &self.svc {
                svc.show_notification_with_options(a.message.as_str(), &opts)
                    .unwrap();
            } else {
                let n = N::new_with_options(a.message.as_str(), &opts).unwrap();
                seed::set_timeout(Box::new(move || n.close()), 4000);
            }
        } else {
            self.update(ah);
        }
    }
}

pub(crate) async fn init() -> Result<Msg, Msg> {
    if N::permission() == NP::Default {
        let rq_p = N::request_permission().unwrap();
        JsFuture::from(rq_p).await.unwrap();
    }

    if N::permission() == NP::Granted {
        let svc_p = seed::window()
            .navigator()
            .service_worker()
            .register("/static/notification.sw.js");

        JsFuture::from(svc_p)
            .await
            .map(|v| Msg::SetupNotification(Ok(v)))
            .map_err(|v| Msg::SetupNotification(Err(v)))
    } else {
        Err(Msg::SetupNotification(Err(JsValue::from("notifications not permitted"))))
    }
}
