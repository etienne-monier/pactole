use pactole_core::{
    Amounts, ExpenseGrouping, RegisterEntry, Report, Valuator, balance_sheet, compare_reports,
    expense_breakdown_with_options, income_statement, periods_have_same_length, register,
};

use crate::backend::{LedgerReader, LedgerSnapshot};
use crate::capabilities::{BackendCapabilities, Capability};
use crate::diagnostics::Diagnostic;
use crate::errors::{ApplicationError, InvalidQuery, WriteError};
use crate::queries::{
    BalanceQuery, DiagnosticsQuery, ExpensesQuery, IncomeQuery, RegisterQuery, ValuationRequest,
};
use crate::results::{
    BalanceResult, DiagnosticsResult, ExpensesResult, IncomeResult, RegisterResult,
};
use crate::writer::{LedgerWriter, WriteRequest, WriteResult, WriteTransactionRequest};

/// Application use cases backed by a ledger reader.
pub struct Application<R> {
    /// Backend used for read operations.
    reader: R,
}

/// Application use cases that can persist changes through a writer.
pub struct WritableApplication<R, W> {
    /// Backend used to load snapshots.
    reader: R,
    /// Backend used to persist writes.
    writer: W,
}

impl<R: LedgerReader, W: LedgerWriter> WritableApplication<R, W> {
    /// Creates a writable application service.
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }

    /// Returns the writer capabilities.
    pub fn capabilities(&self) -> BackendCapabilities {
        self.writer.capabilities()
    }

    /// Validates and writes one transaction through the backend writer.
    pub fn write_transaction(
        &mut self,
        request: WriteTransactionRequest,
    ) -> Result<WriteResult, ApplicationError> {
        if !self.writer.capabilities().supports(Capability::Write) {
            return Err(ApplicationError::CapabilityUnavailable {
                capability: Capability::Write,
            });
        }
        if !self
            .writer
            .capabilities()
            .supports(Capability::Transactions)
        {
            return Err(ApplicationError::CapabilityUnavailable {
                capability: Capability::Transactions,
            });
        }

        let snapshot = self.reader.snapshot()?;
        let (journal, diagnostics) = match snapshot {
            LedgerSnapshot::Ready {
                journal,
                revision: _,
                diagnostics,
            } => (journal, diagnostics),
            LedgerSnapshot::Unavailable { diagnostics } => {
                return Err(ApplicationError::LedgerUnavailable { diagnostics });
            }
        };
        if request.options.validation == crate::ValidationPolicy::RejectWarnings
            && diagnostics.iter().any(Diagnostic::is_warning)
        {
            return Err(ApplicationError::WarningsRejected { diagnostics });
        }

        let mut candidate = journal;
        candidate.entries.push(pactole_core::Entry::Transaction(
            request.transaction.clone(),
        ));
        let validation = pactole_core::validate_journal_all(candidate);
        if !validation.issues.is_empty() {
            return Err(ApplicationError::Write(WriteError::ValidationFailed {
                diagnostics: validation
                    .issues
                    .into_iter()
                    .map(|issue| Diagnostic {
                        source: None,
                        span: None,
                        severity: crate::DiagnosticSeverity::Error,
                        origin: crate::DiagnosticOrigin::Business,
                        code: issue.error.code(),
                        message: issue.error.to_string(),
                    })
                    .collect(),
            }));
        }

        self.writer
            .write(WriteRequest::Transaction(request))
            .map_err(ApplicationError::from)
    }
}

impl<R: LedgerReader> Application<R> {
    /// Creates an application service around `reader`.
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Returns the capabilities advertised by the reader.
    pub fn capabilities(&self) -> BackendCapabilities {
        self.reader.capabilities()
    }

    /// Returns diagnostics produced by the backend.
    pub fn diagnostics(
        &self,
        query: DiagnosticsQuery,
    ) -> Result<DiagnosticsResult, ApplicationError> {
        self.require_read()?;
        let snapshot = self.reader.snapshot()?;
        let diagnostics = filter_diagnostics(snapshot.diagnostics(), query.include_warnings);
        Ok(DiagnosticsResult { diagnostics })
    }

    /// Builds a balance sheet.
    pub fn balance(&self, query: BalanceQuery) -> Result<BalanceResult, ApplicationError> {
        let (journal, diagnostics) = self.ready_journal()?;
        validate_valuation(&journal, query.valuation.as_ref())?;

        let mut report =
            balance_sheet(&journal, query.as_of, &query.classification, query.grouping);
        value_report(&mut report, &journal, query.valuation.as_ref())?;

        Ok(BalanceResult {
            report,
            diagnostics,
        })
    }

    /// Builds an income statement.
    pub fn income(&self, query: IncomeQuery) -> Result<IncomeResult, ApplicationError> {
        let (journal, diagnostics) = self.ready_journal()?;
        validate_valuation(&journal, query.valuation.as_ref())?;

        let mut report = income_statement(
            &journal,
            query.period,
            &query.classification,
            query.grouping,
        );
        value_report(&mut report, &journal, query.valuation.as_ref())?;

        Ok(IncomeResult {
            report,
            diagnostics,
        })
    }

    /// Builds an expenses report and, optionally, a period comparison.
    pub fn expenses(&self, query: ExpensesQuery) -> Result<ExpensesResult, ApplicationError> {
        validate_expenses_query(&query)?;
        let (journal, diagnostics) = self.ready_journal()?;
        validate_valuation(&journal, query.valuation.as_ref())?;

        let mut current = expense_breakdown_with_options(
            &journal,
            query.period,
            &query.classification,
            query.grouping,
            &query.options,
        );
        value_report(&mut current, &journal, query.valuation.as_ref())?;

        let comparison = query
            .comparison
            .map(|comparison| -> Result<_, ApplicationError> {
                let mut previous = expense_breakdown_with_options(
                    &journal,
                    comparison.period,
                    &query.classification,
                    query.grouping,
                    &query.options,
                );
                value_report(&mut previous, &journal, query.valuation.as_ref())?;
                Ok(compare_reports(
                    &current,
                    &previous,
                    comparison.direction,
                    comparison.mode,
                ))
            })
            .transpose()?;

        Ok(ExpensesResult {
            current,
            comparison,
            diagnostics,
        })
    }

    /// Builds a register listing, applying valuation to amounts and balances.
    pub fn register(&self, query: RegisterQuery) -> Result<RegisterResult, ApplicationError> {
        let (journal, diagnostics) = self.ready_journal()?;
        validate_valuation(&journal, query.valuation.as_ref())?;

        let mut entries = register(&journal, &query.filter);
        let valuation = value_register(&mut entries, &journal, query.valuation.as_ref())?;

        Ok(RegisterResult {
            entries,
            valuation,
            diagnostics,
        })
    }

    /// Ensures that the configured backend advertises read support.
    fn require_read(&self) -> Result<(), ApplicationError> {
        if self.reader.capabilities().supports(Capability::Read) {
            Ok(())
        } else {
            Err(ApplicationError::CapabilityUnavailable {
                capability: Capability::Read,
            })
        }
    }

    /// Loads a validated journal or converts backend unavailability to an error.
    fn ready_journal(&self) -> Result<(pactole_core::Journal, Vec<Diagnostic>), ApplicationError> {
        self.require_read()?;
        match self.reader.snapshot()? {
            LedgerSnapshot::Ready {
                journal,
                revision: _,
                diagnostics,
            } => Ok((journal, diagnostics)),
            LedgerSnapshot::Unavailable { diagnostics } => {
                Err(ApplicationError::LedgerUnavailable { diagnostics })
            }
        }
    }
}

/// Filters warnings from diagnostics when the query does not request them.
fn filter_diagnostics(diagnostics: &[Diagnostic], include_warnings: bool) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| include_warnings || diagnostic.is_error())
        .cloned()
        .collect()
}

/// Validates combinations of expense grouping, comparison and options.
fn validate_expenses_query(query: &ExpensesQuery) -> Result<(), ApplicationError> {
    if query.comparison.is_some() && matches!(query.grouping, ExpenseGrouping::Month) {
        return Err(InvalidQuery::ComparisonByMonth.into());
    }
    if query.comparison.is_some()
        && (query.options.include_uncategorized || query.options.include_inactive)
    {
        return Err(InvalidQuery::ComparisonWithExpenseOptions.into());
    }
    if let Some(comparison) = &query.comparison
        && !periods_have_same_length(query.period, comparison.period)
    {
        return Err(InvalidQuery::ComparisonPeriodsHaveDifferentLengths.into());
    }
    Ok(())
}

/// Checks that a requested valuation commodity is declared in the journal.
fn validate_valuation(
    journal: &pactole_core::Journal,
    valuation: Option<&ValuationRequest>,
) -> Result<(), ApplicationError> {
    let Some(valuation) = valuation else {
        return Ok(());
    };

    if journal.entries().iter().any(|entry| {
        matches!(
            entry,
            pactole_core::Entry::Commodity(declaration) if declaration.name == valuation.target
        )
    }) {
        Ok(())
    } else {
        Err(InvalidQuery::UndeclaredValuationCommodity(valuation.target.to_string()).into())
    }
}

/// Applies the requested market-price valuation to a report.
fn value_report(
    report: &mut Report,
    journal: &pactole_core::Journal,
    valuation: Option<&ValuationRequest>,
) -> Result<(), ApplicationError> {
    let Some(valuation) = valuation else {
        return Ok(());
    };
    let mut valuator = Valuator::new(journal, valuation.target.clone(), valuation.at);
    report.value_in(&mut valuator)?;
    Ok(())
}

/// Applies the requested valuation to every register entry.
fn value_register(
    entries: &mut [RegisterEntry],
    journal: &pactole_core::Journal,
    valuation: Option<&ValuationRequest>,
) -> Result<Option<pactole_core::Valuation>, ApplicationError> {
    let Some(valuation) = valuation else {
        return Ok(None);
    };
    let mut valuator = Valuator::new(journal, valuation.target.clone(), valuation.at);

    for entry in entries {
        if let Some(amount) = &entry.amount {
            let mut amounts = Amounts::new();
            amounts.add(amount);
            entry.amount =
                valuator
                    .convert_amounts(&amounts)?
                    .iter()
                    .next()
                    .map(|(commodity, number)| pactole_core::Amount {
                        commodity: commodity.clone(),
                        number,
                    });
        }

        let mut balance = Amounts::new();
        for (commodity, number) in &entry.balance {
            balance.add(&pactole_core::Amount {
                commodity: commodity.clone(),
                number: *number,
            });
        }
        entry.balance = valuator
            .convert_amounts(&balance)?
            .iter()
            .map(|(commodity, number)| (commodity.clone(), number))
            .collect();
    }

    Ok(Some(valuator.valuation()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BackendCapabilities, BackendError, LedgerSnapshot, LedgerWriter, Revision, WriteRequest,
        WriteResult, WriteTarget,
    };
    use chrono::NaiveDate;
    use pactole_core::{
        AccountClassification, AccountGrouping, Entry, Journal, Metadata, Open, Transaction,
        TransactionStatus,
    };

    struct FakeReader {
        snapshot: LedgerSnapshot,
    }

    impl LedgerReader for FakeReader {
        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities::read_only()
        }

        fn snapshot(&self) -> Result<LedgerSnapshot, BackendError> {
            Ok(self.snapshot.clone())
        }
    }

    struct FakeWriter;

    impl LedgerWriter for FakeWriter {
        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities::new(true, true, true, false).unwrap()
        }

        fn write(&mut self, request: WriteRequest) -> Result<WriteResult, crate::WriteError> {
            let WriteRequest::Transaction(request) = request;
            Ok(WriteResult {
                entry_id: None,
                revision: Revision(String::from("next")),
                target: request.target,
                diagnostics: vec![],
            })
        }
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
    }

    fn journal() -> Journal {
        Journal {
            entries: vec![Entry::Open(Open {
                date: date(),
                account: pactole_core::AccountName::new("Assets:Checking").unwrap(),
                commodities: vec![],
                description: None,
                meta: Metadata::default(),
            })],
        }
    }

    fn transaction() -> Transaction {
        Transaction {
            date: date(),
            effective_date: None,
            status: TransactionStatus::Uncleared,
            payee: None,
            narration: Some(String::from("write test")),
            postings: vec![],
            tags: vec![],
            links: vec![],
            reference: None,
            meta: Metadata::default(),
        }
    }

    #[test]
    fn balance_delegates_to_core_with_a_fake_reader() {
        let application = Application::new(FakeReader {
            snapshot: LedgerSnapshot::Ready {
                journal: journal(),
                revision: Revision(String::from("initial")),
                diagnostics: vec![],
            },
        });

        let result = application
            .balance(BalanceQuery {
                as_of: date().into(),
                grouping: AccountGrouping::Full,
                classification: AccountClassification::default(),
                valuation: None,
            })
            .unwrap();

        assert_eq!(result.report.title, "Balance sheet");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn unavailable_snapshot_returns_its_diagnostics() {
        let application = Application::new(FakeReader {
            snapshot: LedgerSnapshot::Unavailable {
                diagnostics: vec![],
            },
        });

        let error = application
            .balance(BalanceQuery {
                as_of: date().into(),
                grouping: AccountGrouping::Exact,
                classification: AccountClassification::default(),
                valuation: None,
            })
            .unwrap_err();

        assert!(matches!(error, ApplicationError::LedgerUnavailable { .. }));
    }

    #[test]
    fn writable_application_validates_before_calling_the_writer() {
        let mut application = WritableApplication::new(
            FakeReader {
                snapshot: LedgerSnapshot::Ready {
                    journal: journal(),
                    revision: Revision(String::from("initial")),
                    diagnostics: vec![],
                },
            },
            FakeWriter,
        );

        let result = application
            .write_transaction(crate::WriteTransactionRequest {
                transaction: transaction(),
                target: WriteTarget::Default,
                expected_revision: None,
                options: crate::WriteOptions::default(),
            })
            .unwrap();

        assert_eq!(result.revision, Revision(String::from("next")));
    }

    #[test]
    fn writable_application_rejects_a_read_only_writer() {
        struct ReadOnlyWriter;

        impl LedgerWriter for ReadOnlyWriter {
            fn capabilities(&self) -> BackendCapabilities {
                BackendCapabilities::read_only()
            }

            fn write(&mut self, _request: WriteRequest) -> Result<WriteResult, crate::WriteError> {
                unreachable!("read-only writers must be rejected before writing")
            }
        }

        let mut application = WritableApplication::new(
            FakeReader {
                snapshot: LedgerSnapshot::Unavailable {
                    diagnostics: vec![],
                },
            },
            ReadOnlyWriter,
        );

        let error = application
            .write_transaction(crate::WriteTransactionRequest {
                transaction: transaction(),
                target: WriteTarget::Default,
                expected_revision: None,
                options: crate::WriteOptions::default(),
            })
            .unwrap_err();

        assert!(matches!(
            error,
            ApplicationError::CapabilityUnavailable {
                capability: Capability::Write
            }
        ));
    }
}
