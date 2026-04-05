#[derive(Debug, Clone)]
pub enum RoutingScope {
    Cluster,
    Datacenter {
        dc: String,
        fallback: Option<Box<RoutingScope>>,
    },
    Rack {
        dc: String,
        rack: String,
        fallback: Option<Box<RoutingScope>>,
    },
}

impl RoutingScope {
    pub fn cluster() -> Self {
        Self::Cluster
    }

    pub fn datacenter(dc: String) -> Self {
        Self::Datacenter { dc, fallback: None }
    }

    pub fn rack(dc: String, rack: String) -> Self {
        Self::Rack {
            dc,
            rack,
            fallback: None,
        }
    }

    pub fn with_fallback(self, fallback: RoutingScope) -> Self {
        match self {
            Self::Cluster => self,
            Self::Datacenter { dc, .. } => Self::Datacenter {
                dc,
                fallback: Some(Box::new(fallback)),
            },
            Self::Rack { dc, rack, .. } => Self::Rack {
                dc,
                rack,
                fallback: Some(Box::new(fallback)),
            },
        }
    }
}
