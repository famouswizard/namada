use borsh::{BorshDeserialize, BorshSerialize};
use ibc::apps::nft_transfer::types::msgs::transfer::MsgTransfer as IbcMsgNftTransfer;
use ibc::apps::nft_transfer::types::packet::PacketData as NftPacketData;
use ibc::apps::transfer::types::msgs::transfer::MsgTransfer as IbcMsgTransfer;
use ibc::apps::transfer::types::packet::PacketData;
use ibc::core::channel::types::msgs::PacketMsg;
use ibc::core::channel::types::packet::Packet;
use ibc::core::handler::types::msgs::MsgEnvelope;
use ibc::core::host::types::identifiers::PortId;
use ibc::primitives::proto::Protobuf;
use namada_token::ShieldingTransfer;

/// The different variants of an Ibc message
#[derive(Debug, Clone)]
pub enum IbcMessage {
    /// Ibc Envelop
    Envelope(MsgEnvelope),
    /// Ibc transaprent transfer
    Transfer(MsgTransfer),
    /// NFT transfer
    NftTransfer(MsgNftTransfer),
}

/// IBC transfer message with `Transfer`
#[derive(Debug, Clone)]
pub struct MsgTransfer {
    /// IBC transfer message
    pub message: IbcMsgTransfer,
    /// Shieleded transfer for MASP transaction
    pub transfer: Option<ShieldingTransfer>,
}

impl BorshSerialize for MsgTransfer {
    fn serialize<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        let encoded_msg = self.message.clone().encode_vec();
        let members = (encoded_msg, self.transfer.clone());
        BorshSerialize::serialize(&members, writer)
    }
}

impl BorshDeserialize for MsgTransfer {
    fn deserialize_reader<R: std::io::Read>(
        reader: &mut R,
    ) -> std::io::Result<Self> {
        use std::io::{Error, ErrorKind};
        let (msg, transfer): (Vec<u8>, Option<ShieldingTransfer>) =
            BorshDeserialize::deserialize_reader(reader)?;
        let message = IbcMsgTransfer::decode_vec(&msg)
            .map_err(|err| Error::new(ErrorKind::InvalidData, err))?;
        Ok(Self { message, transfer })
    }
}

/// IBC NFT transfer message with `Transfer`
#[derive(Debug, Clone)]
pub struct MsgNftTransfer {
    /// IBC NFT transfer message
    pub message: IbcMsgNftTransfer,
    /// Shieleded transfer for MASP transaction
    pub transfer: Option<ShieldingTransfer>,
}

impl BorshSerialize for MsgNftTransfer {
    fn serialize<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        let encoded_msg = self.message.clone().encode_vec();
        let members = (encoded_msg, self.transfer.clone());
        BorshSerialize::serialize(&members, writer)
    }
}

impl BorshDeserialize for MsgNftTransfer {
    fn deserialize_reader<R: std::io::Read>(
        reader: &mut R,
    ) -> std::io::Result<Self> {
        use std::io::{Error, ErrorKind};
        let (msg, transfer): (Vec<u8>, Option<ShieldingTransfer>) =
            BorshDeserialize::deserialize_reader(reader)?;
        let message = IbcMsgNftTransfer::decode_vec(&msg)
            .map_err(|err| Error::new(ErrorKind::InvalidData, err))?;
        Ok(Self { message, transfer })
    }
}

/// Extract the memo string from IBC envelope
pub fn extract_memo_from_envelope(envelope: &MsgEnvelope) -> Option<String> {
    match envelope {
        MsgEnvelope::Packet(packet_msg) => match packet_msg {
            PacketMsg::Recv(msg) => {
                extract_memo_from_packet(&msg.packet, false)
            }
            PacketMsg::Ack(msg) => extract_memo_from_packet(&msg.packet, true),
            PacketMsg::Timeout(msg) => {
                extract_memo_from_packet(&msg.packet, true)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Extract the memo string from IBC packet
pub fn extract_memo_from_packet(
    packet: &Packet,
    is_sender: bool,
) -> Option<String> {
    let is_ft_packet = if is_sender {
        packet.port_id_on_a == PortId::transfer()
    } else {
        packet.port_id_on_b == PortId::transfer()
    };

    if is_ft_packet {
        let packet_data =
            serde_json::from_slice::<PacketData>(&packet.data).ok()?;
        if packet_data.memo.as_ref().is_empty() {
            return None;
        }
        Some(packet_data.memo.as_ref().to_string())
    } else {
        let packet_data =
            serde_json::from_slice::<NftPacketData>(&packet.data).ok()?;
        packet_data.memo.map(|s| s.as_ref().to_string())
    }
}
