use std::path::PathBuf;
use std::process::ExitCode;

use chrono::NaiveDate;
use pactole_app::{RegisterQuery, ValuationRequest};
use pactole_config::ValidationOptions;
use pactole_core::{AccountName, RegisterFilter, TransactionStatus};

use crate::commands::{RegisterFormatArg, StatusArg};

#[allow(clippy::too_many_arguments)]
/// Runs the register command with its account and transaction filters.
pub(crate) fn run(
    file: PathBuf,
    account: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    payee: Option<String>,
    narration: Option<String>,
    tag: Option<String>,
    status: Option<StatusArg>,
    format: RegisterFormatArg,
    convert_to: Option<String>,
    at: Option<NaiveDate>,
    validation: &ValidationOptions,
) -> ExitCode {
    let account = match account {
        Some(account) => match AccountName::new(account) {
            Ok(account) => Some(account),
            Err(err) => return failure(err),
        },
        None => None,
    };

    let filter = RegisterFilter {
        account,
        from,
        to,
        payee,
        narration,
        tag,
        status: status.map(TransactionStatus::from),
    };

    let default_at = filter
        .to
        .unwrap_or_else(|| chrono::Local::now().date_naive());
    let valuation = match convert_to {
        Some(target) => Some(ValuationRequest {
            target: match pactole_core::CommodityName::new(target) {
                Ok(target) => target,
                Err(error) => return failure(error),
            },
            at: at.unwrap_or(default_at),
        }),
        None => {
            if at.is_some() {
                return failure("--at requires --convert-to");
            }
            None
        }
    };
    let result = match crate::application::service(&file, validation)
        .register(RegisterQuery { filter, valuation })
    {
        Ok(result) => result,
        Err(error) => return crate::application::report_failure(error),
    };
    crate::diagnostics::print_application(&result.diagnostics);
    let entries = result.entries;
    let valuation = result.valuation;
    match format {
        RegisterFormatArg::Text => {
            if let Some(valuation) = &valuation {
                print!("{}", crate::valuation::render_valuation(valuation));
            }
            for entry in entries {
                let status = match entry.status {
                    TransactionStatus::Uncleared => "?",
                    TransactionStatus::Pending => "!",
                    TransactionStatus::Cleared => "*",
                };
                let description = [entry.payee.as_deref(), entry.narration.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
                let amount = match &entry.amount {
                    Some(amount) => format!("{} {}", amount.number, amount.commodity),
                    None => String::new(),
                };
                let balance = entry
                    .balance
                    .iter()
                    .map(|(commodity, sum)| format!("{sum} {commodity}"))
                    .collect::<Vec<_>>()
                    .join(", ");

                println!(
                    "{}  {}  {:<40}  {:<25}  {:>15}  {:>15}",
                    entry.date, status, description, entry.account, amount, balance
                );
            }
        }
        RegisterFormatArg::Json => {
            let entries = entries
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "date": entry.date.to_string(),
                        "status": match &entry.status {
                            TransactionStatus::Uncleared => "uncleared",
                            TransactionStatus::Pending => "pending",
                            TransactionStatus::Cleared => "cleared",
                        },
                        "payee": entry.payee.as_deref(),
                        "narration": entry.narration.as_deref(),
                        "account": entry.account.as_str(),
                        "amount": entry.amount.as_ref().map(|amount| serde_json::json!({
                            "number": amount.number.to_string(),
                            "commodity": amount.commodity.as_str(),
                        })),
                        "balance": entry.balance.iter().map(|(commodity, number)| {
                            serde_json::json!({
                                "commodity": commodity.as_str(),
                                "number": number.to_string(),
                            })
                        }).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>();
            let entries = serde_json::to_value(entries).unwrap_or_else(|_| serde_json::json!([]));
            let value = if let Some(valuation) = &valuation {
                serde_json::json!({
                    "entries": entries,
                    "valuation": {
                        "currency": valuation.currency.as_str(),
                        "date": valuation.date.to_string(),
                        "prices": valuation.prices.iter().map(|price| serde_json::json!({
                            "base": price.base.as_str(),
                            "quote": price.quote.as_str(),
                            "rate": price.rate.to_string(),
                            "date": price.date.to_string(),
                        })).collect::<Vec<_>>(),
                        "missing": valuation.missing.iter().map(|commodity| commodity.as_str()).collect::<Vec<_>>(),
                    }
                })
            } else {
                entries
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| String::from("[]"))
            );
        }
    }

    ExitCode::SUCCESS
}

/// Prints a register-command error and returns the failure exit code.
fn failure(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
