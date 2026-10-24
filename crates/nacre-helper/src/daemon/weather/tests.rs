use super::*;

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test(flavor = "current_thread")]
async fn topic_contract_and_refresh_validation() -> Result<()> {
    let _test = TEST_LOCK.lock().await;
    let hub = super::super::hub::Hub::new("weather-test".into(), false, Duration::from_millis(1))
        .shared();
    initial(&hub)?;
    let empty: WeatherState = serde_json::from_value(hub.borrow().snapshot(TOPIC).unwrap())?;
    assert!(!empty.health.available);
    assert!(!empty.refreshing);
    assert!(empty.current.is_none());
    let fixture = include_str!("../../../../../protocol/fixtures/core-dashboard-weather.jsonl")
        .lines()
        .next()
        .unwrap();
    let fixture: Value = serde_json::from_str(fixture)?;
    let mut data = fixture["data"]["current"].clone();
    data["checked"] = json!(1800000000.25);
    data["stale"] = json!(false);
    data["error"] = json!("");
    data["latitude"] = json!(60.0);
    data["longitude"] = json!(5.0);
    data["isDay"] = json!(true);
    data["localTime"] = data["local_time"].clone();
    data["feelsLike"] = data["feels_like"].clone();
    data["hours"] = json!([{"time":"2027-01-15T08:00","temperature":12,"code":0,"rain":10}]);
    data["days"] = json!([{"date":"2027-01-15","high":14,"low":6,"code":0,"rain":10,"sunrise":"07","sunset":"17","uv":2}]);
    let mut malformed = data.clone();
    malformed["temperature"] = Value::Null;
    assert!(
        state(malformed).is_err(),
        "A malformed forecast must use stale retention"
    );
    let reading = state(data.clone())?;
    assert!(CoreSnapshot::Weather(Box::new(reading.clone())).valid());
    assert_eq!(reading.current.as_ref().unwrap().hours.order[0].0, "hour-0");
    publish(&hub, &reading)?;
    assert_eq!(location(&hub).unwrap().latitude, 60.0);
    data["stale"] = json!(true);
    data["error"] = json!("Weather unavailable; showing cached conditions");
    let stale = state(data)?;
    assert!(stale.health.available && stale.health.stale);
    assert_eq!(stale.checked, reading.checked);
    assert_eq!(stale.current, reading.current);
    for data in [
        None,
        Some(json!({"stale":true,"error":"Weather unavailable. Set a city or try again."})),
    ] {
        let retained = complete(reading.clone(), data);
        assert_eq!(retained.current, reading.current);
        assert_eq!(retained.checked, reading.checked);
        assert!(retained.health.available && retained.health.stale);
    }
    let mut refreshing = reading.clone();
    refreshing.refreshing = true;
    assert!(!complete(refreshing, None).refreshing);
    assert!(dispatch(hub.clone(), json!({"extra":true})).is_err());
    hub.borrow_mut()
        .publish("settings.store.desktop", json!({"error":null,"data":{}}))?;
    let _guard = LANE
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
        .try_lock_owned()
        .unwrap();
    assert_eq!(dispatch(hub, json!({})).unwrap_err().code, ErrorCode::Busy);
    let safe =
        super::super::hub::Hub::new("weather-safe".into(), true, Duration::from_millis(1)).shared();
    assert_eq!(
        dispatch(safe, json!({})).unwrap_err().code,
        ErrorCode::PermissionDenied
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn startup_manual_refresh_and_settings_changes() -> Result<()> {
    let _test = TEST_LOCK.lock().await;
    tokio::task::LocalSet::new()
        .run_until(async {
            let root = tempfile::tempdir()?;
            let paths = Paths::new(root.path());
            let hub = super::super::hub::Hub::new(
                "weather-refresh".into(),
                false,
                Duration::from_millis(1),
            )
            .shared();
            initial(&hub)?;
            hub.borrow_mut().publish(
                "settings.store.desktop",
                json!({"error":null,"data":{"weatherIpLookup":false}}),
            )?;
            let task = tokio::task::spawn_local(run(hub.clone(), paths.clone()));
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let data = hub.borrow().snapshot(TOPIC).unwrap();
                    if data["health"]["error"].is_object() && data["refreshing"] == false {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await?;
            let before = hub.borrow().topic_seq(TOPIC).unwrap();
            assert!(start(hub.clone(), paths.clone()).is_ok());
            assert_eq!(
                start(hub.clone(), paths.clone()).unwrap_err().code,
                ErrorCode::Busy
            );
            // A setting update during a running refresh queues another attempt and does no network.
            hub.borrow_mut().publish(
                "settings.store.desktop",
                json!({"error":null,"data":{"weatherIpLookup":false,"weatherFahrenheit":true}}),
            )?;
            tokio::time::timeout(Duration::from_secs(2), async {
                while hub.borrow().snapshot(TOPIC).unwrap()["refreshing"] == true
                    || hub.borrow().topic_seq(TOPIC).unwrap() < before + 4
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await?;
            // Advance only the scheduler clock: the production 30-minute timer must
            // start and finish another refresh, still with consent off and no city.
            let before = hub.borrow().topic_seq(TOPIC).unwrap();
            tokio::time::pause();
            tokio::time::advance(INTERVAL).await;
            tokio::time::resume();
            tokio::time::timeout(Duration::from_secs(2), async {
                while hub.borrow().topic_seq(TOPIC).unwrap() < before + 2
                    || hub.borrow().snapshot(TOPIC).unwrap()["refreshing"] == true
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await?;
            // Model an existing worker lane while the settings file becomes invalid.
            let mut active: WeatherState =
                serde_json::from_value(hub.borrow().snapshot(TOPIC).unwrap())?;
            active.refreshing = true;
            publish(&hub, &active)?;
            let guard = LANE.get().unwrap().clone().try_lock_owned().unwrap();
            hub.borrow_mut().publish(
                "settings.store.desktop",
                json!({"error":{"message":"Invalid settings"},"data":{}}),
            )?;
            tokio::time::timeout(Duration::from_secs(2), async {
                while hub.borrow().snapshot(TOPIC).unwrap()["health"]["error"]["message"]
                    != "Weather settings unavailable"
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await?;
            assert_eq!(
                hub.borrow().snapshot(TOPIC).unwrap()["health"]["stale"],
                true
            );
            assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["refreshing"], true);
            drop(guard);
            task.abort();
            let _ = task.await;
            Ok(())
        })
        .await
}
