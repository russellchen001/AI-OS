use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum OfficialCapabilityStatus {
    Available,
    LimitedApprovalRequired,
    OfficialConsumerApiUnavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommerceProviderCapability {
    platform: &'static str,
    oauth: OfficialCapabilityStatus,
    product_search: OfficialCapabilityStatus,
    product_detail: OfficialCapabilityStatus,
    account_price: OfficialCapabilityStatus,
    cart: OfficialCapabilityStatus,
    checkout: OfficialCapabilityStatus,
    payment: OfficialCapabilityStatus,
    order_history: OfficialCapabilityStatus,
    seller_messaging: OfficialCapabilityStatus,
    fallback: &'static str,
    evidence_url: &'static str,
}

pub(crate) fn capability_matrix() -> Vec<CommerceProviderCapability> {
    vec![
        CommerceProviderCapability {
            platform: "eBay",
            oauth: OfficialCapabilityStatus::Available,
            product_search: OfficialCapabilityStatus::Available,
            product_detail: OfficialCapabilityStatus::Available,
            account_price: OfficialCapabilityStatus::LimitedApprovalRequired,
            cart: OfficialCapabilityStatus::LimitedApprovalRequired,
            checkout: OfficialCapabilityStatus::LimitedApprovalRequired,
            payment: OfficialCapabilityStatus::LimitedApprovalRequired,
            order_history: OfficialCapabilityStatus::LimitedApprovalRequired,
            seller_messaging: OfficialCapabilityStatus::LimitedApprovalRequired,
            fallback: "authenticated-browser",
            evidence_url: "https://developer.ebay.com/api-docs/buy/buy-requirements.html",
        },
        CommerceProviderCapability {
            platform: "Amazon Consumer",
            oauth: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            product_search: OfficialCapabilityStatus::LimitedApprovalRequired,
            product_detail: OfficialCapabilityStatus::LimitedApprovalRequired,
            account_price: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            cart: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            checkout: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            payment: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            order_history: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            seller_messaging: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            fallback: "authenticated-browser",
            evidence_url: "https://webservices.amazon.com/paapi5/documentation/",
        },
        CommerceProviderCapability {
            platform: "Taobao Consumer",
            oauth: OfficialCapabilityStatus::LimitedApprovalRequired,
            product_search: OfficialCapabilityStatus::LimitedApprovalRequired,
            product_detail: OfficialCapabilityStatus::LimitedApprovalRequired,
            account_price: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            cart: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            checkout: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            payment: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            order_history: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            seller_messaging: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            fallback: "authenticated-browser",
            evidence_url: "https://open.taobao.com/",
        },
        CommerceProviderCapability {
            platform: "JD Consumer",
            oauth: OfficialCapabilityStatus::LimitedApprovalRequired,
            product_search: OfficialCapabilityStatus::LimitedApprovalRequired,
            product_detail: OfficialCapabilityStatus::LimitedApprovalRequired,
            account_price: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            cart: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            checkout: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            payment: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            order_history: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            seller_messaging: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            fallback: "authenticated-browser",
            evidence_url: "https://open.jd.com/",
        },
        CommerceProviderCapability {
            platform: "Pinduoduo Consumer",
            oauth: OfficialCapabilityStatus::LimitedApprovalRequired,
            product_search: OfficialCapabilityStatus::LimitedApprovalRequired,
            product_detail: OfficialCapabilityStatus::LimitedApprovalRequired,
            account_price: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            cart: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            checkout: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            payment: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            order_history: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            seller_messaging: OfficialCapabilityStatus::OfficialConsumerApiUnavailable,
            fallback: "authenticated-browser",
            evidence_url: "https://open.pinduoduo.com/",
        },
    ]
}

#[tauri::command]
pub(crate) fn list_commerce_provider_capabilities() -> Vec<CommerceProviderCapability> {
    capability_matrix()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consumer_and_merchant_boundaries_are_explicit() {
        let matrix = capability_matrix();
        assert_eq!(matrix.len(), 5);
        let amazon = matrix
            .iter()
            .find(|item| item.platform == "Amazon Consumer")
            .unwrap();
        assert_eq!(
            amazon.checkout,
            OfficialCapabilityStatus::OfficialConsumerApiUnavailable
        );
        let ebay = matrix.iter().find(|item| item.platform == "eBay").unwrap();
        assert_eq!(
            ebay.checkout,
            OfficialCapabilityStatus::LimitedApprovalRequired
        );
    }
}
