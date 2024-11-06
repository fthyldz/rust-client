use std::{sync::Arc, time::Duration};

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, time::interval};
use tokio_tungstenite::{connect_async, tungstenite::{client::IntoClientRequest, protocol::Message}};

#[derive(Serialize, Deserialize, Debug)]
struct ChatMessage {
    msg_type: String,
    content: String,
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Sunucu URL'si    
    let request = "ws://localhost:8080/".into_client_request().unwrap();

    // Sunucuya bağlan"
    println!("Sunucuya bağlanılıyor: {}", request.uri().to_string());
    let (ws_stream, _) = connect_async(request).await?;
    println!("WebSocket bağlantısı kuruldu");

    // Stream'i gönderme ve alma kısımlarına ayır
    let (write, mut read) = ws_stream.split();

    let write = Arc::new(Mutex::new(write));

    // Ping gönderme intervali (her 30 saniyede bir)
    let mut ping_interval = interval(Duration::from_secs(30));

    // Ping task'ı
    let write_clone = Arc::clone(&write);
    let ping_handler = tokio::spawn(async move {
        loop {
            ping_interval.tick().await;
            let mut write = write_clone.lock().await;
            if let Err(e) = write.send(Message::Ping(vec![])).await {
                println!("Ping gönderme hatası: {}", e);
                break;
            }
            println!("Sunucuya ping gönderildi");
        }
    });

    // Kullanıcı girişini işleyecek task
    let write_clone = Arc::clone(&write);
    let user_input = tokio::spawn(async move {
        loop {
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            let input = input.trim();

            if input == "quit" {
                break;
            }

            let chat_msg = ChatMessage {
                msg_type: String::from("chat"),
                content: input.to_string()
            };
            let json_msg = serde_json::to_string(&chat_msg).unwrap();
            // Mesajı sunucuya gönder
            let mut write = write_clone.lock().await;
            write.send(Message::Text(json_msg)).await
                .unwrap_or_else(|e| println!("Mesaj gönderme hatası: {}", e));
        }
    });

    // Sunucudan gelen mesajları işleyecek task
    let write_clone = Arc::clone(&write);
    let server_messages = tokio::spawn(async move {
        let mut last_pong = tokio::time::Instant::now();

        while let Some(msg) = read.next().await {
            match msg {
                Ok(msg) => {
                    match msg {
                        Message::Text(text) => {
                            match serde_json::from_str::<ChatMessage>(&text) {
                                Ok(chat_msg) => {
                                    println!("Sunucudan gelen mesaj: {}", chat_msg.content);
    
                                }
                                Err(e) => {
                                    println!("JSON ayrıştırma hatası: {}", e);
                                }
                            }
                        },
                        Message::Binary(data) => println!("Binary data alındı: {} bytes", data.len()),
                        Message::Ping(ping_data) => {
                            println!("Sunucudan ping alındı, pong gönderiliyor");
                            // Sunucudan gelen ping'e pong ile cevap ver
                            let mut write = write_clone.lock().await;
                            if let Err(e) = write.send(Message::Pong(ping_data)).await {
                                println!("Pong gönderme hatası: {}", e);
                                break;
                            }
                        }
                        Message::Pong(_) => {
                            last_pong = tokio::time::Instant::now();
                            println!("Sunucudan pong alındı");
                        }
                        Message::Close(_) => {
                            println!("Sunucu bağlantıyı kapattı");
                            break;
                        }
                        Message::Frame(_) => println!("Frame alındı"),
                    }
                }
                Err(e) => {
                    println!("Sunucudan mesaj alırken hata: {}", e);
                    break;
                }
            }

            // Eğer son pong'dan bu yana 90 saniye geçtiyse bağlantıyı kes
            if last_pong.elapsed() > Duration::from_secs(90) {
                println!("Sunucu yanıt vermiyor, bağlantı kesiliyor");
                break;
            }
        }
    });

    // Her iki task'ı da bekle
    tokio::select! {
        _ = user_input => println!("Kullanıcı girişi sonlandı"),
        _ = server_messages => println!("Sunucu bağlantısı sonlandı"),
        _ = ping_handler => println!("Ping işleyici sonlandı"),
    }

    Ok(())
}

use webrtc::api::APIBuilder;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::data_channel::data_channel_message::DataChannelMessage;

#[derive(Serialize, Deserialize, Debug)]
struct SignalMessage {
    msg_type: String,
    sdp: Option<String>,
    candidate: Option<String>,
}

async fn create_peer_connection() -> Result<RTCPeerConnection, Box<dyn std::error::Error>> {
    let config = RTCConfiguration::default();
    let api = APIBuilder::new().build();
    
    // PeerConnection oluştur
    let peer_connection = api.new_peer_connection(config).await?;

    // Data Channel oluştur
    let data_channel = peer_connection.create_data_channel("chat", None).await?;

    // Data Channel mesajlarını işleyin
    data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
        Box::pin(async move {
            println!("DataChannel mesajı alındı: {:?}", msg);
        })
    }));

    // ICE adayları ekleme
    peer_connection.on_ice_candidate(Box::new(|candidate: Option<RTCIceCandidate>| {
        Box::pin(async move {
            if let Some(candidate) = candidate {
                // Burada, ICE adayını JSON olarak istemciye gönderin
                println!("ICE adayı alındı: {:?}", candidate);
            }
        })
    }));

    Ok(peer_connection)
}