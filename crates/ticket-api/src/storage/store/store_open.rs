use super::*;

impl TicketStore {
    pub(super) fn normalize_path(path: PathBuf) -> PathBuf {
        #[cfg(windows)]
        {
            let raw = path.to_string_lossy().replace('\\', "/");
            let normalized = raw
                .strip_prefix("//?/")
                .or_else(|| raw.strip_prefix(r"\\?\"))
                .unwrap_or(&raw);
            PathBuf::from(normalized)
        }

        #[cfg(not(windows))]
        {
            path
        }
    }

    pub(super) fn normalize_existing_path(path: &Path) -> PathBuf {
        std::fs::canonicalize(path)
            .map(Self::normalize_path)
            .unwrap_or_else(|_| Self::normalize_path(path.to_path_buf()))
    }

    pub(super) fn resolved_candidate_matches(
        candidate: &Path,
        marker_file: Option<&str>,
    ) -> bool {
        match marker_file {
            Some(marker_file) => candidate.join(marker_file).is_file(),
            None => candidate.is_dir(),
        }
    }

    pub(super) fn resolve_indexed_path(
        &self,
        path: &Path,
        marker_file: Option<&str>,
    ) -> PathBuf {
        if path.is_absolute()
            && Self::resolved_candidate_matches(path, marker_file)
        {
            return Self::normalize_path(path.to_path_buf());
        }

        if Self::resolved_candidate_matches(path, marker_file) {
            return Self::normalize_existing_path(path);
        }

        for base in self.index_root.ancestors() {
            let candidate = base.join(path);
            if Self::resolved_candidate_matches(&candidate, marker_file) {
                return Self::normalize_existing_path(&candidate);
            }
        }

        Self::normalize_path(path.to_path_buf())
    }

    pub(super) fn resolve_ticket_path(
        &self,
        path: &Path,
    ) -> PathBuf {
        self.resolve_indexed_path(path, Some(TICKET_MANIFEST_FILE))
    }

    pub(super) fn resolve_scan_root_path(
        &self,
        path: &Path,
    ) -> PathBuf {
        self.resolve_indexed_path(path, None)
    }

    pub(super) fn normalize_indexed_ticket(
        &self,
        mut indexed: IndexedTicket,
    ) -> IndexedTicket {
        indexed.path = self.resolve_ticket_path(&indexed.path);
        indexed
    }

    pub(super) fn normalize_indexed_tickets(
        &self,
        tickets: Vec<IndexedTicket>,
    ) -> Vec<IndexedTicket> {
        tickets
            .into_iter()
            .map(|ticket| self.normalize_indexed_ticket(ticket))
            .collect()
    }

    /// Open an existing ticket store rooted at `index_root` using built-in schemas.
    ///
    /// Returns [`StorageError::WorkspaceNotFound`] if the workspace has not been
    /// initialized yet. Run `ticket init` to create a new workspace first.
    pub fn open(index_root: &Path) -> Result<Self, StorageError> {
        Self::open_with(index_root, SchemaRegistry::with_builtins())
    }

    /// Open an existing ticket store with a custom schema registry.
    ///
    /// Returns [`StorageError::WorkspaceNotFound`] if the workspace has not been
    /// initialized yet. Use [`TicketStore::init_with`] to create a new workspace.
    pub fn open_with(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<Self, StorageError> {
        let (store, _) = Self::open_with_profiled(index_root, schema_registry)?;
        Ok(store)
    }

    pub fn open_with_profiled(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<(Self, StoreOpenReport), StorageError> {
        let _span_guard = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_open_profiled"
        )
        .entered();
        let overall_started = Instant::now();
        let resolve_started = Instant::now();
        let index_root = workspace::resolve_store_root_from(
            index_root,
            workspace::TICKET_INDEX_DIR,
        );
        let mut report = StoreOpenReport::default();
        report.phase_timings_ms.insert(
            "resolve_store_root_ms".to_string(),
            elapsed_ms(resolve_started),
        );
        let existence_started = Instant::now();
        if !index_root.join("tickets.db").is_file() {
            return Err(StorageError::WorkspaceNotFound { path: index_root });
        }
        report.phase_timings_ms.insert(
            "workspace_exists_check_ms".to_string(),
            elapsed_ms(existence_started),
        );
        let (store, internal_report) =
            Self::open_internal_profiled(index_root, schema_registry)?;
        merge_timings(
            &mut report.phase_timings_ms,
            internal_report.phase_timings_ms,
        );
        report.scan_reports.extend(internal_report.scan_reports);
        report
            .phase_timings_ms
            .insert("open_total_ms".to_string(), elapsed_ms(overall_started));
        emit_store_open_report("ticket_store_open_profiled_complete", &report);
        Ok((store, report))
    }

    /// Initialize a new ticket store rooted at `index_root` using built-in schemas.
    ///
    /// Creates the workspace directory and all required index files. Idempotent:
    /// if the workspace already exists it is opened without error.
    pub fn init(index_root: &Path) -> Result<Self, StorageError> {
        Self::init_with(index_root, SchemaRegistry::with_builtins())
    }

    /// Open an existing ticket store, or initialize and rebuild it when the
    /// local derived index artifacts do not exist yet.
    pub fn open_or_init(index_root: &Path) -> Result<Self, StorageError> {
        Self::open_or_init_with(index_root, SchemaRegistry::with_builtins())
    }

    /// Initialize a new ticket store with a custom schema registry.
    ///
    /// Creates the workspace directory and all required index files. Idempotent:
    /// if the workspace already exists it is opened without error.
    pub fn init_with(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<Self, StorageError> {
        let (store, _) = Self::init_with_profiled(index_root, schema_registry)?;
        Ok(store)
    }

    fn init_with_profiled(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<(Self, StoreOpenReport), StorageError> {
        let _span_guard = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_init_profiled"
        )
        .entered();
        let overall_started = Instant::now();
        let resolve_started = Instant::now();
        let index_root = workspace::resolve_store_root_from(
            index_root,
            workspace::TICKET_INDEX_DIR,
        );
        let mut report = StoreOpenReport {
            initialized_store: true,
            ..Default::default()
        };
        report.phase_timings_ms.insert(
            "resolve_store_root_ms".to_string(),
            elapsed_ms(resolve_started),
        );
        let ensure_started = Instant::now();
        ensure_sqlite_index_root(
            &index_root,
            "tickets.db",
            &["search_index/"],
        )?;
        report.phase_timings_ms.insert(
            "ensure_index_root_ms".to_string(),
            elapsed_ms(ensure_started),
        );
        let (store, internal_report) =
            Self::open_internal_profiled(index_root, schema_registry)?;
        merge_timings(
            &mut report.phase_timings_ms,
            internal_report.phase_timings_ms,
        );
        report.scan_reports.extend(internal_report.scan_reports);
        report
            .phase_timings_ms
            .insert("init_total_ms".to_string(), elapsed_ms(overall_started));
        emit_store_open_report("ticket_store_init_profiled_complete", &report);
        Ok((store, report))
    }

    /// Open an existing ticket store with a custom schema registry, or
    /// initialize and rebuild it when the local derived index artifacts do not
    /// exist yet.
    pub fn open_or_init_with(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<Self, StorageError> {
        let (store, _) =
            Self::open_or_init_with_profiled(index_root, schema_registry)?;
        Ok(store)
    }

    pub fn open_or_init_profiled(
        index_root: &Path
    ) -> Result<(Self, StoreOpenReport), StorageError> {
        Self::open_or_init_with_profiled(
            index_root,
            SchemaRegistry::with_builtins(),
        )
    }

    pub fn open_or_init_with_profiled(
        index_root: &Path,
        schema_registry: SchemaRegistry,
    ) -> Result<(Self, StoreOpenReport), StorageError> {
        let span = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_open_or_init",
            initialized_store = Empty,
        );
        let _span_guard = span.enter();
        let overall_started = Instant::now();
        match Self::open_with_profiled(index_root, schema_registry.clone()) {
            Ok((store, mut report)) => {
                span.record("initialized_store", false);
                report.phase_timings_ms.insert(
                    "open_or_init_total_ms".to_string(),
                    elapsed_ms(overall_started),
                );
                emit_store_open_report(
                    "ticket_store_open_or_init_complete",
                    &report,
                );
                Ok((store, report))
            },
            Err(StorageError::WorkspaceNotFound { .. }) => {
                span.record("initialized_store", true);
                let (store, mut report) =
                    Self::init_with_profiled(index_root, schema_registry)?;
                let initial_scan_started = Instant::now();
                let initial_scan = store.scan(true)?;
                report.phase_timings_ms.insert(
                    "post_init_scan_ms".to_string(),
                    elapsed_ms(initial_scan_started),
                );
                report
                    .scan_reports
                    .insert("post_init_scan".to_string(), initial_scan);
                report.phase_timings_ms.insert(
                    "open_or_init_total_ms".to_string(),
                    elapsed_ms(overall_started),
                );
                emit_store_open_report(
                    "ticket_store_open_or_init_complete",
                    &report,
                );
                Ok((store, report))
            },
            Err(error) => Err(error),
        }
    }

    pub(super) fn open_internal_profiled(
        index_root: std::path::PathBuf,
        schema_registry: SchemaRegistry,
    ) -> Result<(Self, StoreOpenReport), StorageError> {
        let _span_guard = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_open_internal"
        )
        .entered();
        let overall_started = Instant::now();
        let normalize_started = Instant::now();
        let index_root = Self::normalize_existing_path(&index_root);
        let mut report = StoreOpenReport::default();
        report.phase_timings_ms.insert(
            "normalize_existing_path_ms".to_string(),
            elapsed_ms(normalize_started),
        );
        let db_path = index_root.join("tickets.db");
        let search_dir = index_root.join("search_index");

        let sqlite_started = Instant::now();
        let index = RedbIndexStore::open(&db_path)?;
        report.phase_timings_ms.insert(
            "open_sqlite_index_ms".to_string(),
            elapsed_ms(sqlite_started),
        );
        let search_started = Instant::now();
        let search = match TantivySearchIndex::open_or_create(&search_dir) {
            Ok(search) => search,
            Err(_) => {
                let _ = std::fs::remove_dir_all(&search_dir);
                TantivySearchIndex::open_or_create(&search_dir)?
            },
        };
        report.phase_timings_ms.insert(
            "open_search_index_ms".to_string(),
            elapsed_ms(search_started),
        );

        let store = Self {
            index,
            search,
            schema_registry,
            index_root: index_root.clone(),
            hook: OnceLock::new(),
        };
        let add_root_started = Instant::now();
        store.add_scan_root(ScanRoot {
            path: index_root.join("tickets"),
            label: "tickets".to_string(),
        })?;
        report.phase_timings_ms.insert(
            "add_default_scan_root_ms".to_string(),
            elapsed_ms(add_root_started),
        );
        let bootstrap_started = Instant::now();
        let bootstrap =
            store.bootstrap_empty_index_from_manifests_profiled()?;
        report.phase_timings_ms.insert(
            "bootstrap_empty_index_ms".to_string(),
            elapsed_ms(bootstrap_started),
        );
        merge_prefixed_timings(
            &mut report.phase_timings_ms,
            "bootstrap",
            bootstrap.phase_timings_ms,
        );
        report.scan_reports.extend(bootstrap.scan_reports);
        report.phase_timings_ms.insert(
            "open_internal_total_ms".to_string(),
            elapsed_ms(overall_started),
        );
        emit_store_open_report("ticket_store_open_internal_complete", &report);
        Ok((store, report))
    }

    fn bootstrap_empty_index_from_manifests_profiled(
        &self
    ) -> Result<StoreOpenReport, StorageError> {
        let _span_guard = tracing::debug_span!(
            target: STORE_TRACE_TARGET,
            "ticket_store_bootstrap_empty_index"
        )
        .entered();
        let mut report = StoreOpenReport::default();
        let count_started = Instant::now();
        if self.count_tickets()? > 0 {
            report.phase_timings_ms.insert(
                "count_tickets_ms".to_string(),
                elapsed_ms(count_started),
            );
            let search_probe_started = Instant::now();
            if self
                .search
                .search(&crate::model::query::Expr::And(Vec::new()), 1)
                .map(|results| results.is_empty())
                .unwrap_or(true)
            {
                report.phase_timings_ms.insert(
                    "search_probe_ms".to_string(),
                    elapsed_ms(search_probe_started),
                );
                let bootstrap_scan_started = Instant::now();
                let scan_report = self.scan(true)?;
                report.phase_timings_ms.insert(
                    "scan_existing_index_ms".to_string(),
                    elapsed_ms(bootstrap_scan_started),
                );
                report.scan_reports.insert(
                    "bootstrap_existing_index_scan".to_string(),
                    scan_report,
                );
                return Ok(report);
            }
            report.phase_timings_ms.insert(
                "search_probe_ms".to_string(),
                elapsed_ms(search_probe_started),
            );
            return Ok(report);
        }
        report
            .phase_timings_ms
            .insert("count_tickets_ms".to_string(), elapsed_ms(count_started));

        let roots_started = Instant::now();
        for root in self.list_scan_roots()? {
            report.phase_timings_ms.insert(
                "list_scan_roots_ms".to_string(),
                elapsed_ms(roots_started),
            );
            let manifest_probe_started = Instant::now();
            if scan_root_has_ticket_manifests(&root.path)? {
                report.phase_timings_ms.insert(
                    "manifest_probe_ms".to_string(),
                    elapsed_ms(manifest_probe_started),
                );
                let bootstrap_scan_started = Instant::now();
                let scan_report = self.scan(true)?;
                report.phase_timings_ms.insert(
                    "scan_manifest_bootstrap_ms".to_string(),
                    elapsed_ms(bootstrap_scan_started),
                );
                report
                    .scan_reports
                    .insert("bootstrap_manifest_scan".to_string(), scan_report);
                break;
            }
            report.phase_timings_ms.insert(
                "manifest_probe_ms".to_string(),
                elapsed_ms(manifest_probe_started),
            );
        }

        if !report.phase_timings_ms.contains_key("list_scan_roots_ms") {
            report.phase_timings_ms.insert(
                "list_scan_roots_ms".to_string(),
                elapsed_ms(roots_started),
            );
        }

        Ok(report)
    }
}
