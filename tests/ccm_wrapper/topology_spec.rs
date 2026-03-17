//! Tools for creating topology passed to cluster creation.
//!
//! ```no_run
//! let topology = TopologySpecBuilder::new()
//!     .datacenter(DatacenterSpec::new().rack(1).rack(1))
//!     .datacenter(DatacenterSpec::new().rack(1).rack(2).rack(3))
//!     .datacenter(DatacenterSpec::new().rack(1))
//!     .build();
//! ```

pub(crate) struct TopologySpec {
    pub(crate) datacenters: Vec<DatacenterSpec>,
}

pub(crate) struct DatacenterSpec {
    pub(crate) racks: Vec<usize>,
}

impl DatacenterSpec {
    pub(crate) fn new() -> Self {
        Self { racks: Vec::new() }
    }

    pub(crate) fn rack(mut self, node_count: usize) -> Self {
        self.racks.push(node_count);
        self
    }
}

pub(crate) struct TopologySpecBuilder {
    datacenters: Vec<DatacenterSpec>,
}

impl TopologySpecBuilder {
    pub(crate) fn new() -> Self {
        Self {
            datacenters: Vec::new(),
        }
    }

    pub(crate) fn datacenter(mut self, dc: DatacenterSpec) -> Self {
        self.datacenters.push(dc);
        self
    }

    pub(crate) fn build(self) -> anyhow::Result<TopologySpec> {
        Self::validate(&self.datacenters)?;
        Ok(TopologySpec {
            datacenters: self.datacenters,
        })
    }

    fn validate(datacenters: &[DatacenterSpec]) -> anyhow::Result<()> {
        anyhow::ensure!(
            !datacenters.is_empty(),
            "Topology must have at least one datacenter"
        );
        let mut total_nodes_count: usize = 0;
        for (dc_idx, dc) in datacenters.iter().enumerate() {
            anyhow::ensure!(
                !dc.racks.is_empty(),
                "Datacenter dc{} must have at least one rack",
                dc_idx + 1
            );
            for (rack_idx, &node_count) in dc.racks.iter().enumerate() {
                anyhow::ensure!(
                    node_count >= 1,
                    "Rack RAC{} in dc{} must have at least one node",
                    rack_idx + 1,
                    dc_idx + 1
                );
                total_nodes_count += node_count;
            }
        }

        // Every node needs its own ip, so the limit is max value of the last octet.
        anyhow::ensure!(
            total_nodes_count <= usize::from(u8::MAX),
            "Too many nodes in cluster"
        );

        Ok(())
    }
}
